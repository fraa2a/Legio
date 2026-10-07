use std::ffi::OsString;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{Emitter, Manager};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceLink {
    url: Option<String>,
    error: Option<String>,
}

#[derive(Default)]
pub struct SourceLinks {
    requests: Mutex<Vec<SourceLink>>,
    registration_error: Mutex<Option<String>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingLinks {
    requests: Vec<SourceLink>,
    registration_error: Option<String>,
}

pub fn parse(value: &str) -> Result<String, String> {
    if value.len() > 8192 || value.chars().any(char::is_control) {
        return Err("Invalid Legio source link".to_owned());
    }
    let link = reqwest::Url::parse(value).map_err(|_| "Invalid Legio source link".to_owned())?;
    if link.scheme() != "legio"
        || link.host_str() != Some("add-source")
        || !matches!(link.path(), "" | "/")
        || link.port().is_some()
        || !link.username().is_empty()
        || link.password().is_some()
        || link.fragment().is_some()
    {
        return Err("Expected legio://add-source?url=<encoded HTTPS URL>".to_owned());
    }
    let mut pairs = link.query_pairs();
    let Some((key, value)) = pairs.next() else {
        return Err("The source link has no URL".to_owned());
    };
    if key != "url" || pairs.next().is_some() {
        return Err("The source link must contain exactly one URL parameter".to_owned());
    }
    crate::legio_source::validate_manifest_url(&value)
        .map(|url| url.to_string())
        .map_err(|error| error.to_string())
}

impl SourceLinks {
    pub fn enqueue(&self, arguments: impl IntoIterator<Item = OsString>) -> Result<bool, String> {
        let mut requests = self
            .requests
            .lock()
            .map_err(|_| "Source links are unavailable".to_owned())?;
        let mut added = false;
        for argument in arguments {
            let Some(value) = argument.to_str() else {
                continue;
            };
            if !value
                .get(..6)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("legio:"))
            {
                continue;
            }
            if requests.len() >= 32 {
                return Err("Too many pending source links".to_owned());
            }
            let request = match parse(value) {
                Ok(url) => SourceLink {
                    url: Some(url),
                    error: None,
                },
                Err(error) => SourceLink {
                    url: None,
                    error: Some(error),
                },
            };
            requests.push(request);
            added = true;
        }
        Ok(added)
    }

    pub fn registration_failed(&self, error: String) -> Result<(), String> {
        *self
            .registration_error
            .lock()
            .map_err(|_| "Source link registration is unavailable".to_owned())? = Some(error);
        Ok(())
    }

    pub fn take(&self) -> Result<PendingLinks, String> {
        Ok(PendingLinks {
            requests: std::mem::take(
                &mut *self
                    .requests
                    .lock()
                    .map_err(|_| "Source links are unavailable".to_owned())?,
            ),
            registration_error: self
                .registration_error
                .lock()
                .map_err(|_| "Source link registration is unavailable".to_owned())?
                .clone(),
        })
    }
}

pub fn forward(app: &tauri::AppHandle, arguments: &[String]) -> Result<(), String> {
    if app
        .state::<SourceLinks>()
        .enqueue(arguments.iter().skip(1).map(OsString::from))?
    {
        app.emit("legio:source-link", ())
            .map_err(|error| format!("Could not publish source link: {error}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_encoded_https_urls_and_preserves_nested_query_parameters() {
        assert_eq!(
            parse("legio://add-source?url=https%3A%2F%2Fcatalogo.example%2Fgames.json").unwrap(),
            "https://catalogo.example/games.json"
        );
        assert_eq!(
            parse(
                "legio://add-source?url=https%3A%2F%2Fcatalogo.example%2Fgames.json%3Fa%3D1%26b%3D2"
            )
            .unwrap(),
            "https://catalogo.example/games.json?a=1&b=2"
        );
    }

    #[test]
    fn rejects_ambiguous_or_invalid_links() {
        for value in [
            "legio://add-source",
            "legio://other?url=https://example.invalid/a.json",
            "legio://add-source/path?url=https://example.invalid/a.json",
            "legio://add-source?url=http://example.invalid/a.json",
            "legio://add-source?url=file:///tmp/a.json",
            "legio://add-source?url=https://user:pass@example.invalid/a.json",
            "legio://add-source?url=https://example.invalid/a.json&url=https://example.invalid/b.json",
            "legio://add-source?url=https://example.invalid/a.json&other=1",
            "legio://add-source?url=https://example.invalid/a.json#fragment",
        ] {
            assert!(parse(value).is_err(), "{value}");
        }
    }

    #[test]
    fn startup_and_forwarded_links_are_drained_once() {
        let links = SourceLinks::default();
        assert!(
            links
                .enqueue([
                    OsString::from("--minimized"),
                    OsString::from("legio://add-source?url=https://example.invalid/a.json")
                ])
                .unwrap()
        );
        assert!(
            links
                .enqueue([OsString::from("legio://add-source?url=bad")])
                .unwrap()
        );
        let pending = links.take().unwrap();
        assert_eq!(pending.requests.len(), 2);
        assert!(pending.requests[0].url.is_some());
        assert!(pending.requests[1].error.is_some());
        assert!(links.take().unwrap().requests.is_empty());
    }
}
