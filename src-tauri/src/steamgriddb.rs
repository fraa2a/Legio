//! SteamGridDB artwork search for the game page.
//!
//! The API key lives only in the system keychain and is never returned to the frontend.
//! Every response is untrusted: sizes are capped, only `https` URLs on the SteamGridDB
//! CDN hosts are accepted and chosen images go through the custom artwork pipeline.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    network::{NetworkError, NetworkState},
    secret_store::SecretStore,
};

pub(crate) const MAX_API_BYTES: usize = 512 * 1024;
pub(crate) const KEY_ACCOUNT: &str = "steamgriddb-api-key";
const API_BASE: &str = "https://www.steamgriddb.com/api/v2";
// The API hands out image URLs on these hosts only.
const IMAGE_HOSTS: [&str; 2] = ["cdn.steamgriddb.com", "cdn2.steamgriddb.com"];
const MAX_RESULTS: usize = 40;
const MAX_GAMES: usize = 20;
const MAX_QUERY_CHARS: usize = 100;
const MAX_NAME_CHARS: usize = 120;
const MIN_KEY_CHARS: usize = 8;
const MAX_KEY_CHARS: usize = 128;
// Pixel sizes of landscape grids that fit the banner slot.
const BANNER_GRID_DIMENSIONS: &str = "460x215,920x430";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ListingKind {
    Grid,
    Hero,
    Icon,
}

impl ListingKind {
    fn endpoint(self) -> &'static str {
        match self {
            Self::Grid => "grids",
            Self::Hero => "heroes",
            Self::Icon => "icons",
        }
    }
}

/// Custom artwork slot that receives the chosen image.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Slot {
    Icon,
    Banner,
}

impl Slot {
    pub(crate) fn artwork_kind(self) -> crate::game_artwork::ArtworkKind {
        match self {
            Self::Icon => crate::game_artwork::ArtworkKind::Icon,
            Self::Banner => crate::game_artwork::ArtworkKind::Banner,
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SgdbGame {
    id: u64,
    name: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SgdbImage {
    id: u64,
    url: String,
    thumb_url: String,
    width: u32,
    height: u32,
}

#[derive(Deserialize)]
struct Envelope<T> {
    #[serde(default)]
    success: bool,
    data: Option<T>,
}

#[derive(Deserialize)]
struct RawGame {
    id: u64,
    #[serde(default)]
    name: String,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct RawImage {
    id: u64,
    url: String,
    thumb: String,
    width: u32,
    height: u32,
    mime: String,
    nsfw: bool,
    humor: bool,
}

struct ApiKey(String);

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiKey(<redacted>)")
    }
}

/// Keychain-backed API key plus the managed state the commands share.
#[derive(Clone)]
pub(crate) struct SteamGridDb {
    keys: Arc<dyn SecretStore>,
}

impl SteamGridDb {
    pub(crate) fn new(keys: Arc<dyn SecretStore>) -> Self {
        Self { keys }
    }

    pub(crate) fn has_key(&self) -> Result<bool, String> {
        self.keys
            .get()
            .map(|key| key.is_some())
            .map_err(|error| error.to_string())
    }

    pub(crate) fn save_key(&self, raw: &str) -> Result<(), String> {
        let key = raw.trim();
        let chars = key.chars().count();
        if !(MIN_KEY_CHARS..=MAX_KEY_CHARS).contains(&chars)
            || !key.chars().all(|c| c.is_ascii_graphic())
        {
            return Err("The SteamGridDB API key is not valid.".to_owned());
        }
        self.keys.set(key).map_err(|error| error.to_string())
    }

    pub(crate) fn clear_key(&self) -> Result<(), String> {
        self.keys.clear().map_err(|error| error.to_string())
    }

    fn key(&self) -> Result<ApiKey, String> {
        match self.keys.get() {
            Ok(Some(key)) => Ok(ApiKey(key)),
            Ok(None) => Err("Set a SteamGridDB API key in Settings first.".to_owned()),
            Err(error) => Err(error.to_string()),
        }
    }

    /// Reads the key from the keychain (blocking) and runs the authenticated search.
    pub(crate) async fn search_games(
        &self,
        network: &NetworkState,
        query: Option<String>,
        steam_app_id: Option<u32>,
    ) -> Result<Vec<SgdbGame>, String> {
        let key = self.read_key().await?;
        let mut games = Vec::new();
        if let Some(app_id) = steam_app_id.filter(|id| *id > 0) {
            let url = api_url(&["games", "steam", &app_id.to_string()], &[])?;
            match network.steamgriddb_api(&url, &key.0).await {
                Ok(bytes) => games.extend(parse_game(&bytes)?),
                Err(NetworkError::Http { status: 404 }) => {}
                Err(error) => return Err(describe(&error)),
            }
        }
        if let Some(term) = clean_query(query.as_deref()) {
            let url = api_url(&["search", "autocomplete", &term], &[])?;
            let bytes = network
                .steamgriddb_api(&url, &key.0)
                .await
                .map_err(|error| describe(&error))?;
            for game in parse_games(&bytes)? {
                if !games.iter().any(|known| known.id == game.id) {
                    games.push(game);
                }
            }
        }
        games.truncate(MAX_GAMES);
        Ok(games)
    }

    pub(crate) async fn list_images(
        &self,
        network: &NetworkState,
        game_id: u64,
        kind: ListingKind,
    ) -> Result<Vec<SgdbImage>, String> {
        let key = self.read_key().await?;
        let mut query = vec![("nsfw", "false"), ("humor", "false"), ("types", "static")];
        if kind == ListingKind::Grid {
            query.push(("dimensions", BANNER_GRID_DIMENSIONS));
        }
        let url = api_url(&[kind.endpoint(), "game", &game_id.to_string()], &query)?;
        match network.steamgriddb_api(&url, &key.0).await {
            Ok(bytes) => parse_images(&bytes),
            Err(NetworkError::Http { status: 404 }) => Ok(Vec::new()),
            Err(error) => Err(describe(&error)),
        }
    }

    async fn read_key(&self) -> Result<ApiKey, String> {
        let this = self.clone();
        tauri::async_runtime::spawn_blocking(move || this.key())
            .await
            .map_err(|error| format!("SteamGridDB key task failed: {error}"))?
    }
}

/// Downloads a chosen image. The URL is revalidated because it comes back from the frontend.
pub(crate) async fn download_image(network: &NetworkState, url: &str) -> Result<Vec<u8>, String> {
    let url = image_url(url)?;
    network
        .steamgriddb_image(&url, crate::game_artwork::MAX_ARTWORK_BYTES)
        .await
        .map_err(|error| match error {
            NetworkError::TooLarge => {
                "This image is larger than the 2 MiB artwork limit. Choose a smaller one."
                    .to_owned()
            }
            other => describe(&other),
        })
}

fn api_url(segments: &[&str], query: &[(&str, &str)]) -> Result<reqwest::Url, String> {
    let mut url = reqwest::Url::parse(API_BASE).map_err(|error| error.to_string())?;
    url.path_segments_mut()
        .map_err(|()| "Invalid SteamGridDB address".to_owned())?
        .extend(segments);
    if !query.is_empty() {
        url.query_pairs_mut().extend_pairs(query);
    }
    Ok(url)
}

fn image_url(value: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(value)
        .map_err(|_| "The SteamGridDB image address is not valid.".to_owned())?;
    let allowed = url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url
            .host_str()
            .is_some_and(|host| IMAGE_HOSTS.contains(&host));
    if allowed {
        Ok(url)
    } else {
        Err("The SteamGridDB image address is not allowed.".to_owned())
    }
}

fn clean_query(query: Option<&str>) -> Option<String> {
    let term: String = query?
        .chars()
        .map(|c| if c == '/' || c.is_control() { ' ' } else { c })
        .take(MAX_QUERY_CHARS)
        .collect();
    let term = term.trim();
    (!term.is_empty()).then(|| term.to_owned())
}

fn describe(error: &NetworkError) -> String {
    match error {
        NetworkError::Http { status: 401 | 403 } => {
            "SteamGridDB rejected the API key. Check it in Settings.".to_owned()
        }
        NetworkError::Http { status: 429 } => {
            "SteamGridDB is limiting requests. Try again in a minute.".to_owned()
        }
        other => format!("Could not reach SteamGridDB: {other}"),
    }
}

fn data<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<Option<T>, String> {
    if bytes.len() > MAX_API_BYTES {
        return Err("SteamGridDB returned a response larger than the size limit.".to_owned());
    }
    let envelope: Envelope<T> = serde_json::from_slice(bytes)
        .map_err(|error| format!("SteamGridDB returned an unreadable response: {error}"))?;
    if envelope.success {
        Ok(envelope.data)
    } else {
        Err("SteamGridDB reported an error.".to_owned())
    }
}

fn game(raw: RawGame) -> Option<SgdbGame> {
    let name: String = raw.name.trim().chars().take(MAX_NAME_CHARS).collect();
    (raw.id > 0 && !name.is_empty()).then_some(SgdbGame { id: raw.id, name })
}

fn parse_game(bytes: &[u8]) -> Result<Option<SgdbGame>, String> {
    Ok(data::<RawGame>(bytes)?.and_then(game))
}

fn parse_games(bytes: &[u8]) -> Result<Vec<SgdbGame>, String> {
    Ok(data::<Vec<RawGame>>(bytes)?
        .unwrap_or_default()
        .into_iter()
        .filter_map(game)
        .collect())
}

fn parse_images(bytes: &[u8]) -> Result<Vec<SgdbImage>, String> {
    Ok(data::<Vec<RawImage>>(bytes)?
        .unwrap_or_default()
        .into_iter()
        .filter(|raw| {
            !raw.nsfw
                && !raw.humor
                && matches!(raw.mime.as_str(), "image/png" | "image/jpeg" | "image/webp")
        })
        .filter_map(|raw| {
            let url = image_url(&raw.url).ok()?;
            let thumb = image_url(&raw.thumb).ok()?;
            Some(SgdbImage {
                id: raw.id,
                url: url.into(),
                thumb_url: thumb.into(),
                width: raw.width,
                height: raw.height,
            })
        })
        .take(MAX_RESULTS)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret_store::tests::FakeKeychain;

    fn service(keychain: FakeKeychain) -> SteamGridDb {
        SteamGridDb::new(Arc::new(keychain))
    }

    #[test]
    fn key_can_be_set_checked_and_cleared() {
        let service = service(FakeKeychain::default());
        assert_eq!(service.has_key(), Ok(false));
        service.save_key("  0123456789abcdef  ").unwrap();
        assert_eq!(service.has_key(), Ok(true));
        assert_eq!(service.key().unwrap().0, "0123456789abcdef");
        service.clear_key().unwrap();
        assert_eq!(service.has_key(), Ok(false));
        assert!(service.key().is_err());
    }

    #[test]
    fn invalid_keys_are_not_stored() {
        let service = service(FakeKeychain::default());
        for key in [
            "",
            "short",
            "has space inside1",
            "line\nbreak12345",
            &"x".repeat(129),
        ] {
            assert!(service.save_key(key).is_err(), "{key:?}");
        }
        assert_eq!(service.has_key(), Ok(false));
    }

    #[test]
    fn missing_keychain_is_reported_without_fallback() {
        let service = service(FakeKeychain::unavailable());
        for message in [
            service.has_key().unwrap_err(),
            service.save_key("0123456789abcdef").unwrap_err(),
            service.clear_key().unwrap_err(),
            service.key().unwrap_err(),
        ] {
            assert!(message.contains("No system keychain"), "{message}");
        }
    }

    #[test]
    fn key_is_never_serialized_or_printed() {
        let service = service(FakeKeychain::default());
        service.save_key("0123456789abcdef").unwrap();
        let key = service.key().unwrap();
        assert!(!format!("{key:?}").contains("0123456789abcdef"));
        // The only values that reach the frontend carry no key field.
        let game = serde_json::to_string(&SgdbGame {
            id: 1,
            name: "A".into(),
        })
        .unwrap();
        assert!(!game.contains("key") && !game.contains("0123456789abcdef"));
    }

    #[test]
    fn image_urls_are_limited_to_the_cdn_hosts() {
        assert!(image_url("https://cdn2.steamgriddb.com/grid/a.png").is_ok());
        assert!(image_url("https://cdn.steamgriddb.com/hero/a.jpg").is_ok());
        for url in [
            "http://cdn2.steamgriddb.com/grid/a.png",
            "https://www.steamgriddb.com/grid/a.png",
            "https://cdn2.steamgriddb.com.evil.test/a.png",
            "https://evil.test/cdn2.steamgriddb.com/a.png",
            "https://user@cdn2.steamgriddb.com/a.png",
            "https://cdn2.steamgriddb.com:8443/a.png",
            "https://127.0.0.1/a.png",
            "file:///etc/passwd",
            "not a url",
        ] {
            assert!(image_url(url).is_err(), "{url}");
        }
    }

    #[test]
    fn parses_games_and_drops_unusable_rows() {
        let body = br#"{"success":true,"data":[
            {"id":5,"name":"Portal","types":["steam"],"verified":true},
            {"id":0,"name":"Zero"},
            {"id":6,"name":"   "},
            {"id":7,"name":"Portal 2"}]}"#;
        let games = parse_games(body).unwrap();
        assert_eq!(games.iter().map(|game| game.id).collect::<Vec<_>>(), [5, 7]);
        assert_eq!(
            parse_game(br#"{"success":true,"data":{"id":9,"name":"X"}}"#)
                .unwrap()
                .unwrap()
                .id,
            9
        );
        assert_eq!(parse_game(br#"{"success":true}"#).unwrap(), None);
    }

    #[test]
    fn parses_images_and_filters_untrusted_entries() {
        let body = br#"{"success":true,"data":[
            {"id":1,"url":"https://cdn2.steamgriddb.com/hero/a.png","thumb":"https://cdn2.steamgriddb.com/thumb/a.jpg","width":1920,"height":620,"mime":"image/png","nsfw":false,"humor":false},
            {"id":2,"url":"https://evil.test/a.png","thumb":"https://cdn2.steamgriddb.com/thumb/b.jpg","mime":"image/png"},
            {"id":3,"url":"https://cdn2.steamgriddb.com/hero/c.png","thumb":"https://evil.test/c.jpg","mime":"image/png"},
            {"id":4,"url":"https://cdn2.steamgriddb.com/hero/d.png","thumb":"https://cdn2.steamgriddb.com/thumb/d.jpg","mime":"image/png","nsfw":true},
            {"id":5,"url":"https://cdn2.steamgriddb.com/icon/e.ico","thumb":"https://cdn2.steamgriddb.com/thumb/e.jpg","mime":"image/vnd.microsoft.icon"},
            {"id":6,"url":"https://cdn2.steamgriddb.com/hero/f.webp","thumb":"https://cdn2.steamgriddb.com/thumb/f.jpg","mime":"image/webp"}]}"#;
        let images = parse_images(body).unwrap();
        assert_eq!(
            images.iter().map(|image| image.id).collect::<Vec<_>>(),
            [1, 6]
        );
        assert_eq!((images[0].width, images[0].height), (1920, 620));
    }

    #[test]
    fn caps_results_and_rejects_malformed_or_failed_responses() {
        let row = r#"{"id":1,"url":"https://cdn2.steamgriddb.com/a.png","thumb":"https://cdn2.steamgriddb.com/t.jpg","mime":"image/png"}"#;
        let body = format!(
            r#"{{"success":true,"data":[{}]}}"#,
            vec![row; 100].join(",")
        );
        assert_eq!(parse_images(body.as_bytes()).unwrap().len(), MAX_RESULTS);
        assert!(parse_images(b"not json").is_err());
        assert!(parse_images(br#"{"success":false,"errors":["Bad key"]}"#).is_err());
        assert!(parse_images(&vec![b' '; MAX_API_BYTES + 1]).is_err());
    }

    #[test]
    fn builds_encoded_api_urls() {
        let url = api_url(&["search", "autocomplete", "a b?x#"], &[("nsfw", "false")]).unwrap();
        assert_eq!(
            url.as_str(),
            "https://www.steamgriddb.com/api/v2/search/autocomplete/a%20b%3Fx%23?nsfw=false"
        );
        assert_eq!(
            clean_query(Some("  Half/Life\n2 ")).as_deref(),
            Some("Half Life 2")
        );
        assert_eq!(clean_query(Some("   ")), None);
        assert_eq!(clean_query(None), None);
    }

    #[test]
    fn describes_key_and_rate_limit_failures() {
        assert!(describe(&NetworkError::Http { status: 401 }).contains("API key"));
        assert!(describe(&NetworkError::Http { status: 429 }).contains("limiting"));
        assert!(describe(&NetworkError::Timeout).contains("Could not reach"));
    }
}
