//! Validation of web addresses taken from package metadata and readmes.

const MAX_URL_LENGTH: usize = 2048;

/// A plain `http` or `https` address with a host and no credentials.
/// Package authors control these values, so nothing else is ever opened.
pub(crate) fn parse_external_url(value: &str) -> Option<reqwest::Url> {
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_URL_LENGTH {
        return None;
    }
    reqwest::Url::parse(value).ok().filter(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_plain_web_addresses() {
        for url in [
            "https://example.com/a?b=1#c",
            "http://example.com",
            " https://example.com/x ",
        ] {
            assert!(parse_external_url(url).is_some(), "{url}");
        }
        for url in [
            "",
            "javascript:alert(1)",
            "file:///C:/Windows",
            "data:text/html,x",
            "mailto:a@example.com",
            "ftp://example.com/",
            "https://user:pass@example.com/",
            "https://user@example.com/",
            "/relative/path",
            "#anchor",
            "not a url",
        ] {
            assert!(parse_external_url(url).is_none(), "{url}");
        }
        assert!(parse_external_url(&format!("https://example.com/{}", "a".repeat(3000))).is_none());
    }
}
