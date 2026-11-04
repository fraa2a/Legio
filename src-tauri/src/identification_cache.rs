use std::collections::{HashMap, HashSet};

use serde::Deserialize;
use sha2::{Digest, Sha256};

// Placeholder: must point to the public HTTPS endpoint of the Cloudflare R2 bucket
// that holds the cache files, one per source link.
const BASE_URL: &str = "https://source-cache.legio.invalid/v1/";
pub(crate) const MAX_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const TIMEOUT_SECONDS: u64 = 15;
const VERSION: u32 = 1;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CacheFile {
    version: u32,
    content_sha256: String,
    matches: HashMap<String, u32>,
    unmatched: Vec<String>,
}

/// Identification results published for one source link. Keys are SHA-256 hashes of raw
/// download titles, so the file never contains source content.
#[derive(Debug)]
pub(crate) struct IdentificationCache {
    content_sha256: String,
    matches: HashMap<String, u32>,
    unmatched: HashSet<String>,
}

/// How the titles of a source split between cached answers and local lookups.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Resolution {
    pub matches: HashMap<String, u32>,
    pub unmatched: HashSet<String>,
    pub pending: Vec<String>,
}

pub(crate) fn content_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn title_hash(title: &str) -> String {
    content_hash(title.as_bytes())
}

fn is_title_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(crate) fn cache_url(source_url: &str) -> Result<String, String> {
    Ok(format!(
        "{BASE_URL}{}.json",
        crate::game_library::source_hash(source_url)?
    ))
}

pub(crate) fn parse(bytes: &[u8]) -> Result<IdentificationCache, String> {
    if bytes.len() > MAX_BYTES {
        return Err("Identification cache exceeds the size limit".into());
    }
    let file: CacheFile = serde_json::from_slice(bytes)
        .map_err(|error| format!("Invalid identification cache: {error}"))?;
    if file.version != VERSION {
        return Err(format!(
            "Unsupported identification cache version {}",
            file.version
        ));
    }
    if !is_title_hash(&file.content_sha256) {
        return Err("Identification cache has an invalid content hash".into());
    }
    if !file.matches.keys().all(|key| is_title_hash(key))
        || !file.unmatched.iter().all(|key| is_title_hash(key))
    {
        return Err("Identification cache has an invalid title hash".into());
    }
    if file.matches.values().any(|id| *id == 0) {
        return Err("Identification cache contains an invalid app id".into());
    }
    let unmatched: HashSet<String> = file.unmatched.into_iter().collect();
    if file.matches.keys().any(|title| unmatched.contains(title)) {
        return Err("Identification cache lists a title as both matched and unmatched".into());
    }
    Ok(IdentificationCache {
        content_sha256: file.content_sha256,
        matches: file.matches,
        unmatched,
    })
}

impl IdentificationCache {
    pub(crate) fn describes(&self, content_hash: &str) -> bool {
        self.content_sha256 == content_hash
    }
}

/// Splits `titles` into cached answers and the titles that still need a local lookup.
/// Titles in `known` keep their match, and cache entries for titles not in the source are ignored.
pub(crate) fn resolve<'a>(
    cache: Option<&IdentificationCache>,
    titles: impl IntoIterator<Item = &'a str>,
    known: &HashMap<String, u32>,
) -> Resolution {
    let mut resolution = Resolution::default();
    for title in titles {
        if known.contains_key(title) {
            continue;
        }
        let hash = title_hash(title);
        match cache {
            Some(cache) if cache.matches.contains_key(&hash) => {
                resolution
                    .matches
                    .insert(title.to_owned(), cache.matches[&hash]);
            }
            Some(cache) if cache.unmatched.contains(&hash) => {
                resolution.unmatched.insert(title.to_owned());
            }
            _ => resolution.pending.push(title.to_owned()),
        }
    }
    resolution
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "https://example.test/hydra.json";
    const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn file(matches: &str, unmatched: &str) -> String {
        format!(
            r#"{{"version":1,"contentSha256":"{HASH}","generatedAt":"2026-11-04T00:00:00Z","matches":{{{matches}}},"unmatched":[{unmatched}]}}"#
        )
    }

    fn valid() -> String {
        file(
            &format!(r#""{}":400"#, title_hash("Portal")),
            &format!(r#""{}""#, title_hash("Unknown")),
        )
    }

    fn cache() -> IdentificationCache {
        parse(valid().as_bytes()).unwrap()
    }

    fn titles(values: &[&str]) -> Vec<String> {
        values.iter().map(|title| (*title).to_owned()).collect()
    }

    #[test]
    fn parses_a_valid_file() {
        let cache = cache();
        assert_eq!(cache.matches[&title_hash("Portal")], 400);
        assert!(cache.unmatched.contains(&title_hash("Unknown")));
        assert!(cache.describes(HASH));
        assert!(!cache.describes(&"0".repeat(64)));
    }

    #[test]
    fn rejects_invalid_files() {
        let portal = title_hash("Portal");
        let unknown = title_hash("Unknown");
        for (bytes, reason) in [
            (valid().replace("\"version\":1", "\"version\":2"), "version"),
            (valid().replace(HASH, "abc"), "content hash"),
            (valid().replace(":400", ":0"), "app id"),
            (valid().replace(&portal, "Portal"), "title key"),
            (
                valid().replace(&portal, &portal.to_uppercase()),
                "uppercase key",
            ),
            (valid().replace(&unknown, "Unknown"), "unmatched key"),
            (valid().replace(&unknown, &portal), "both lists"),
            ("{".to_owned(), "JSON"),
        ] {
            assert!(parse(bytes.as_bytes()).is_err(), "{reason}");
        }
        let oversized = vec![b' '; MAX_BYTES + 1];
        assert!(parse(&oversized).is_err());
        assert!(parse(file("", "").as_bytes()).is_ok());
    }

    #[test]
    fn stores_no_source_content() {
        assert!(!valid().contains("Portal"));
        assert!(!valid().contains(URL));
    }

    #[test]
    fn names_the_file_after_the_source_hash() {
        let url = cache_url(URL).unwrap();
        assert!(url.starts_with(BASE_URL));
        assert!(url.ends_with(".json"));
        assert_eq!(url.len(), BASE_URL.len() + 64 + ".json".len());
        assert!(cache_url("http://example.test/a.json").is_err());
    }

    #[test]
    fn hashes_raw_bytes() {
        assert_eq!(
            content_hash(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn applies_cached_answers_and_leaves_the_diff_pending() {
        let source = titles(&["Portal", "Unknown", "New Game"]);
        let resolution = resolve(
            Some(&cache()),
            source.iter().map(String::as_str),
            &HashMap::new(),
        );
        assert_eq!(
            resolution.matches,
            HashMap::from([("Portal".to_owned(), 400)])
        );
        assert_eq!(resolution.unmatched, HashSet::from(["Unknown".to_owned()]));
        assert_eq!(resolution.pending, ["New Game"]);
    }

    #[test]
    fn ignores_cache_entries_for_titles_outside_the_source() {
        let source = titles(&["Other"]);
        let resolution = resolve(
            Some(&cache()),
            source.iter().map(String::as_str),
            &HashMap::new(),
        );
        assert!(resolution.matches.is_empty());
        assert!(resolution.unmatched.is_empty());
        assert_eq!(resolution.pending, ["Other"]);
    }

    #[test]
    fn known_matches_take_precedence_over_the_cache() {
        let known = HashMap::from([("Portal".to_owned(), 620)]);
        let source = titles(&["Portal", "Unknown"]);
        let resolution = resolve(Some(&cache()), source.iter().map(String::as_str), &known);
        assert!(resolution.matches.is_empty());
        assert_eq!(resolution.unmatched, HashSet::from(["Unknown".to_owned()]));
        assert!(resolution.pending.is_empty());
    }

    #[test]
    fn looks_up_everything_without_a_cache() {
        let known = HashMap::from([("Portal".to_owned(), 400)]);
        let source = titles(&["Portal", "Other"]);
        let resolution = resolve(None, source.iter().map(String::as_str), &known);
        assert_eq!(resolution.pending, ["Other"]);
        assert!(resolution.matches.is_empty());
    }
}
