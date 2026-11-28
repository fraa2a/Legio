use std::ffi::OsString;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{Emitter, Manager};
#[cfg(not(target_os = "linux"))]
use tauri_plugin_deep_link::DeepLinkExt;

use crate::appearance::AppearanceProfile;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceLink {
    url: Option<String>,
    theme: Option<AppearanceProfile>,
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

fn parse_link(value: &str, host: &str) -> Result<reqwest::Url, String> {
    if value.len() > 8192 || value.chars().any(char::is_control) {
        return Err("Invalid Legio link".to_owned());
    }
    let link = reqwest::Url::parse(value).map_err(|_| "Invalid Legio link".to_owned())?;
    if link.scheme() != "legio"
        || link.host_str() != Some(host)
        || !matches!(link.path(), "" | "/")
        || link.port().is_some()
        || !link.username().is_empty()
        || link.password().is_some()
        || link.fragment().is_some()
    {
        return Err(format!("Expected a legio://{host} link"));
    }
    Ok(link)
}

fn single_parameter(link: &reqwest::Url, name: &str) -> Result<String, String> {
    let mut pairs = link.query_pairs();
    let Some((key, value)) = pairs.next() else {
        return Err(format!("The link has no {name} parameter"));
    };
    if key != name || pairs.next().is_some() {
        return Err(format!(
            "The link must contain exactly one {name} parameter"
        ));
    }
    Ok(value.into_owned())
}

pub fn parse(value: &str) -> Result<String, String> {
    let link = parse_link(value, "add-source")?;
    crate::legio_source::validate_manifest_url(&single_parameter(&link, "url")?)
        .map(|url| url.to_string())
        .map_err(|error| error.to_string())
}

pub fn parse_theme(value: &str) -> Result<AppearanceProfile, String> {
    let link = parse_link(value, "theme")?;
    AppearanceProfile::from_share_code(&single_parameter(&link, "code")?)
}

fn is_theme_link(value: &str) -> bool {
    reqwest::Url::parse(value).is_ok_and(|link| link.host_str() == Some("theme"))
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
            let request = if is_theme_link(value) {
                match parse_theme(value) {
                    Ok(theme) => SourceLink {
                        url: None,
                        theme: Some(theme),
                        error: None,
                    },
                    Err(error) => SourceLink {
                        url: None,
                        theme: None,
                        error: Some(error),
                    },
                }
            } else {
                match parse(value) {
                    Ok(url) => SourceLink {
                        url: Some(url),
                        theme: None,
                        error: None,
                    },
                    Err(error) => SourceLink {
                        url: None,
                        theme: None,
                        error: Some(error),
                    },
                }
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

pub fn register(app: &tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    let result = crate::scheme_handler::register(app, "legio");
    #[cfg(not(target_os = "linux"))]
    let result = app
        .deep_link()
        .register_all()
        .map_err(|error| error.to_string());
    result.map_err(|error| format!("Could not register Legio links: {error}"))
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
    fn theme_links_carry_a_validated_profile() {
        let profile: AppearanceProfile =
            serde_json::from_str(r#"{"version":1,"theme":"violet","appearance":{}}"#).unwrap();
        let code = crate::appearance::create_theme_code(profile.clone()).unwrap();
        let link = format!("legio://theme?code={code}");
        assert_eq!(parse_theme(&link).unwrap(), profile);
        for value in [
            "legio://theme",
            "legio://theme?code=",
            "legio://theme?code=LT9.e30",
            "legio://theme?code=nonsense",
            "legio://theme?url=LT1.e30",
            "legio://theme/path?code=LT1.e30",
            "legio://theme?code=LT1.e30&code=LT1.e30",
            "legio://add-source?url=https://example.invalid/a.json",
        ] {
            assert!(parse_theme(value).is_err(), "{value}");
        }
        let links = SourceLinks::default();
        assert!(links.enqueue([OsString::from(link)]).unwrap());
        let pending = links.take().unwrap();
        assert_eq!(pending.requests[0].theme, Some(profile));
        assert!(pending.requests[0].url.is_none() && pending.requests[0].error.is_none());
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
        assert_eq!(
            pending.requests[0].url.as_deref(),
            Some("https://example.invalid/a.json")
        );
        assert!(pending.requests[0].error.is_none());
        assert!(pending.requests[1].url.is_none());
        assert!(pending.requests[1].error.is_some());
        assert!(links.take().unwrap().requests.is_empty());
    }
}
