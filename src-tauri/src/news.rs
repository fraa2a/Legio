use std::{
    collections::HashSet,
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::DateTime;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::{
    database::{Database, DatabaseState},
    network::NetworkState,
};

pub const URL: &str = "https://source.taxphobia.top/news.json";
pub const MAX_BYTES: usize = 512 * 1024;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Feed {
    schema_version: u32,
    generated_at: String,
    items: Vec<Article>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Article {
    id: String,
    published_at: String,
    title: Translation,
    summary: Translation,
    body: Translation,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Translation {
    it: String,
    en: String,
}

#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Snapshot {
    feed: Option<Feed>,
    cached_at: Option<i64>,
    warning: Option<String>,
}

fn timestamp(value: &str) -> Result<(), String> {
    let parsed =
        DateTime::parse_from_rfc3339(value).map_err(|_| "News timestamp must be RFC3339 UTC.")?;
    if parsed.offset().local_minus_utc() != 0 || !value.ends_with('Z') {
        return Err("News timestamp must be RFC3339 UTC.".to_owned());
    }
    Ok(())
}

fn text(value: &Translation, limit: usize, multiline: bool) -> Result<(), String> {
    for text in [&value.it, &value.en] {
        if text.trim().is_empty()
            || text.chars().count() > limit
            || text.chars().any(|character| {
                character.is_control() && !(multiline && matches!(character, '\n' | '\r' | '\t'))
            })
        {
            return Err(format!(
                "News text must be nonempty and at most {limit} characters without invalid controls."
            ));
        }
    }
    Ok(())
}

pub(crate) fn parse(bytes: &[u8]) -> Result<Feed, String> {
    if bytes.len() > MAX_BYTES {
        return Err("News feed exceeds 512 KiB.".to_owned());
    }
    let feed: Feed =
        serde_json::from_slice(bytes).map_err(|error| format!("Invalid news JSON: {error}"))?;
    if feed.schema_version != 1 || feed.items.len() > 100 {
        return Err("Unsupported news schema or too many articles.".to_owned());
    }
    timestamp(&feed.generated_at)?;
    let mut ids = HashSet::new();
    for article in &feed.items {
        if article.id.is_empty()
            || article.id.len() > 64
            || !article
                .id
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            || !ids.insert(&article.id)
        {
            return Err("News IDs must be unique lowercase identifiers.".to_owned());
        }
        timestamp(&article.published_at)?;
        text(&article.title, 160, false)?;
        text(&article.summary, 600, false)?;
        text(&article.body, 12000, true)?;
    }
    Ok(feed)
}

fn read_cache(database: &Database) -> Result<Snapshot, String> {
    let row: Option<String> = database.with_connection(|connection| {
        connection
            .query_row(
                "SELECT value FROM settings WHERE key = 'news_cache'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())
    })?;
    let Some(raw) = row else {
        return Ok(Snapshot::default());
    };
    if raw.len() > MAX_BYTES + 4096 {
        return Err("Cached news exceeds the size limit.".to_owned());
    }
    let snapshot: Snapshot =
        serde_json::from_str(&raw).map_err(|error| format!("Invalid cached news: {error}"))?;
    if let Some(feed) = &snapshot.feed {
        parse(&serde_json::to_vec(feed).map_err(|error| error.to_string())?)?;
    }
    Ok(snapshot)
}

pub async fn cached(app: AppHandle) -> Result<Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        read_cache(app.state::<DatabaseState>().database()?)
    })
    .await
    .map_err(|error| error.to_string())?
}

pub async fn refresh(app: AppHandle, state: &NetworkState) -> Result<Snapshot, String> {
    let fetched = state
        .news()
        .await
        .map_err(|error| format!("Could not fetch news: {error:?}"))
        .and_then(|bytes| parse(&bytes));
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<DatabaseState>();
        let database = state.database()?;
        let fresh = fetched.and_then(|feed| {
            let cached_at = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|error| error.to_string())?.as_secs() as i64;
            let snapshot = Snapshot { feed: Some(feed), cached_at: Some(cached_at), warning: None };
            let raw = serde_json::to_string(&snapshot).map_err(|error| error.to_string())?;
            database.with_connection(|connection| {
                connection.execute("INSERT INTO settings (key, value) VALUES ('news_cache', ?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [raw])
                    .map(|_| ()).map_err(|error| error.to_string())
            })?;
            Ok(snapshot)
        });
        match fresh {
            Ok(snapshot) => Ok(snapshot),
            Err(error) => {
                let mut snapshot = read_cache(database)?;
                snapshot.warning = Some(error);
                Ok(snapshot)
            }
        }
    }).await.map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    fn document() -> serde_json::Value {
        serde_json::json!({"schemaVersion":1,"generatedAt":"2026-10-04T12:00:00Z","items":[{
            "id":"welcome","publishedAt":"2026-10-04T12:00:00Z",
            "title":{"it":"Benvenuto","en":"Welcome"},"summary":{"it":"Novità","en":"News"},
            "body":{"it":"Testo\ncompleto","en":"Full\narticle"}}]})
    }
    #[test]
    fn validates_bilingual_news_and_rejects_invalid_feeds() {
        let valid = document();
        assert!(parse(&serde_json::to_vec(&valid).unwrap()).is_ok());
        for invalid in [
            {
                let mut value = valid.clone();
                value["schemaVersion"] = 2.into();
                value
            },
            {
                let mut value = valid.clone();
                value["items"][0]["title"]["en"] = "".into();
                value
            },
            {
                let mut value = valid.clone();
                value["items"][0]["publishedAt"] = "invalid".into();
                value
            },
            {
                let mut value = valid.clone();
                value["items"][0]["url"] = "javascript:alert(1)".into();
                value
            },
            {
                let mut value = valid.clone();
                value["items"]
                    .as_array_mut()
                    .unwrap()
                    .push(valid["items"][0].clone());
                value
            },
        ] {
            assert!(parse(&serde_json::to_vec(&invalid).unwrap()).is_err());
        }
        assert!(parse(&vec![b' '; MAX_BYTES + 1]).is_err());
    }
    #[test]
    fn cache_survives_database_reopen() {
        let root = std::env::temp_dir().join(format!("legio-news-{}", uuid::Uuid::new_v4()));
        let state = DatabaseState::new(Ok(root.clone()));
        let snapshot = Snapshot {
            feed: Some(parse(&serde_json::to_vec(&document()).unwrap()).unwrap()),
            cached_at: Some(100),
            warning: None,
        };
        state
            .database()
            .unwrap()
            .with_connection(|connection| {
                connection
                    .execute(
                        "INSERT INTO settings VALUES ('news_cache', ?1)",
                        [serde_json::to_string(&snapshot).unwrap()],
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        drop(state);
        let reopened = DatabaseState::new(Ok(root.clone()));
        assert_eq!(
            read_cache(reopened.database().unwrap())
                .unwrap()
                .feed
                .unwrap()
                .items[0]
                .id,
            "welcome"
        );
        drop(reopened);
        std::fs::remove_dir_all(root).unwrap();
    }
}
