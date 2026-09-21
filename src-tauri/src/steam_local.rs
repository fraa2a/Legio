use std::borrow::Cow;
use std::{env, fmt, fs, path::PathBuf};

use serde::Serialize;

pub const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
const MAX_OBJECT_DEPTH: usize = 32;

/// Metadata declared by an app manifest, not proof that its files are installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstalledSteamGame {
    pub app_id: u32,
    pub name: String,
    /// A single directory component; filesystem containment still needs checking.
    pub install_dir: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ManifestDiagnostic {
    InputTooLarge,
    InvalidUtf8,
    Malformed { offset: usize },
    NestingTooDeep,
    MissingField(&'static str),
    InvalidField(&'static str),
    DuplicateField(&'static str),
}

impl fmt::Display for ManifestDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputTooLarge => write!(
                formatter,
                "Steam manifest exceeds {MAX_MANIFEST_BYTES} bytes"
            ),
            Self::InvalidUtf8 => formatter.write_str("Steam manifest is not valid UTF-8"),
            Self::Malformed { offset } => {
                write!(formatter, "Malformed Steam KeyValues at byte {offset}")
            }
            Self::NestingTooDeep => {
                formatter.write_str("Steam manifest object nesting is too deep")
            }
            Self::MissingField(field) => write!(formatter, "Steam manifest is missing {field}"),
            Self::InvalidField(field) => write!(formatter, "Steam manifest has invalid {field}"),
            Self::DuplicateField(field) => write!(formatter, "Steam manifest repeats {field}"),
        }
    }
}

impl std::error::Error for ManifestDiagnostic {}

/// Parses one quoted KeyValues AppState object, including unknown nested fields.
/// Input size and nesting are bounded before allocating records or recursing.
pub fn parse_app_manifest(bytes: &[u8]) -> Result<InstalledSteamGame, ManifestDiagnostic> {
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(ManifestDiagnostic::InputTooLarge);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| ManifestDiagnostic::InvalidUtf8)?;
    let mut parser = Parser {
        text,
        offset: usize::from(text.starts_with('\u{feff}')) * 3,
    };
    if !matches!(parser.next()?, Some(Token::Text(root)) if root.eq_ignore_ascii_case("AppState"))
        || !matches!(parser.next()?, Some(Token::Open))
    {
        return Err(parser.malformed());
    }
    let mut fields = Fields::default();
    parser.object(1, &mut fields)?;
    if parser.next()?.is_some() {
        return Err(parser.malformed());
    }
    let app_id = fields
        .app_id
        .ok_or(ManifestDiagnostic::MissingField("appid"))?;
    let app_id = if !app_id.is_empty() && app_id.bytes().all(|byte| byte.is_ascii_digit()) {
        app_id.parse::<u32>().ok().filter(|id| *id > 0)
    } else {
        None
    }
    .ok_or(ManifestDiagnostic::InvalidField("appid"))?;
    let name = fields
        .name
        .ok_or(ManifestDiagnostic::MissingField("name"))?;
    if name.trim().is_empty() || name.chars().any(char::is_control) {
        return Err(ManifestDiagnostic::InvalidField("name"));
    }
    let install_dir = fields
        .install_dir
        .ok_or(ManifestDiagnostic::MissingField("installdir"))?;
    if install_dir.trim().is_empty()
        || matches!(install_dir.as_ref(), "." | "..")
        || install_dir
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '/' | '\\' | ':'))
    {
        return Err(ManifestDiagnostic::InvalidField("installdir"));
    }
    Ok(InstalledSteamGame {
        app_id,
        name: name.into_owned(),
        install_dir: install_dir.into_owned(),
    })
}

#[derive(Default)]
struct Fields<'a> {
    app_id: Option<Cow<'a, str>>,
    name: Option<Cow<'a, str>>,
    install_dir: Option<Cow<'a, str>>,
}

enum Token<'a> {
    Text(Cow<'a, str>),
    Open,
    Close,
}

struct Parser<'a> {
    text: &'a str,
    offset: usize,
}

impl<'a> Parser<'a> {
    fn malformed(&self) -> ManifestDiagnostic {
        ManifestDiagnostic::Malformed {
            offset: self.offset,
        }
    }

    fn next(&mut self) -> Result<Option<Token<'a>>, ManifestDiagnostic> {
        let bytes = self.text.as_bytes();
        loop {
            while bytes.get(self.offset).is_some_and(u8::is_ascii_whitespace) {
                self.offset += 1;
            }
            if bytes.get(self.offset..self.offset + 2) != Some(b"//") {
                break;
            }
            while bytes.get(self.offset).is_some_and(|byte| *byte != b'\n') {
                self.offset += 1;
            }
        }
        let Some(byte) = bytes.get(self.offset) else {
            return Ok(None);
        };
        self.offset += 1;
        match byte {
            b'{' => Ok(Some(Token::Open)),
            b'}' => Ok(Some(Token::Close)),
            b'"' => self.quoted().map(|text| Some(Token::Text(text))),
            _ => Err(self.malformed()),
        }
    }

    fn quoted(&mut self) -> Result<Cow<'a, str>, ManifestDiagnostic> {
        let bytes = self.text.as_bytes();
        let mut start = self.offset;
        let mut decoded: Option<String> = None;
        while let Some(&byte) = bytes.get(self.offset) {
            match byte {
                b'"' => {
                    let end = self.offset;
                    self.offset += 1;
                    return Ok(match decoded {
                        Some(mut text) => {
                            text.push_str(&self.text[start..end]);
                            Cow::Owned(text)
                        }
                        None => Cow::Borrowed(&self.text[start..end]),
                    });
                }
                b'\\' => {
                    let escaped = match bytes.get(self.offset + 1) {
                        Some(b'"') => Some('"'),
                        Some(b'\\') => Some('\\'),
                        Some(b'n') => Some('\n'),
                        Some(b'r') => Some('\r'),
                        Some(b't') => Some('\t'),
                        None => return Err(self.malformed()),
                        _ => None,
                    };
                    if let Some(escaped) = escaped {
                        let text = decoded.get_or_insert_with(String::new);
                        text.push_str(&self.text[start..self.offset]);
                        text.push(escaped);
                        self.offset += 2;
                        start = self.offset;
                        continue;
                    }
                }
                0..=31 => return Err(self.malformed()),
                _ => {}
            }
            self.offset += 1;
        }
        Err(self.malformed())
    }

    fn object(&mut self, depth: usize, fields: &mut Fields<'a>) -> Result<(), ManifestDiagnostic> {
        if depth > MAX_OBJECT_DEPTH {
            return Err(ManifestDiagnostic::NestingTooDeep);
        }
        loop {
            let key = match self.next()? {
                Some(Token::Close) => return Ok(()),
                Some(Token::Text(key)) => key,
                _ => return Err(self.malformed()),
            };
            let target = if depth == 1 && key.eq_ignore_ascii_case("appid") {
                Some((&mut fields.app_id, "appid"))
            } else if depth == 1 && key.eq_ignore_ascii_case("name") {
                Some((&mut fields.name, "name"))
            } else if depth == 1 && key.eq_ignore_ascii_case("installdir") {
                Some((&mut fields.install_dir, "installdir"))
            } else {
                None
            };
            match (self.next()?, target) {
                (Some(Token::Text(value)), Some((slot, field))) => {
                    if slot.is_some() {
                        return Err(ManifestDiagnostic::DuplicateField(field));
                    }
                    *slot = Some(value);
                }
                (Some(Token::Text(_)), None) => {}
                (Some(Token::Open), None) => self.object(depth + 1, fields)?,
                (Some(Token::Open), Some((_, field))) => {
                    return Err(ManifestDiagnostic::InvalidField(field));
                }
                _ => return Err(self.malformed()),
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamScan {
    pub games: Vec<InstalledSteamGame>,
    pub diagnostics: Vec<String>,
}

pub fn scan_default_installations() -> SteamScan {
    let Some(home) = env::var_os("HOME") else {
        return SteamScan {
            games: Vec::new(),
            diagnostics: vec!["home directory is unavailable".to_owned()],
        };
    };
    let candidates = [
        PathBuf::from(&home).join(".local/share/Steam/steamapps"),
        PathBuf::from(&home).join(".steam/steam/steamapps"),
        PathBuf::from(&home).join(".steam/root/steamapps"),
    ];
    let mut games = Vec::new();
    let mut diagnostics = Vec::new();
    for steamapps in candidates {
        let Ok(entries) = fs::read_dir(&steamapps) else {
            continue;
        };
        for entry in entries.flatten() {
            let filename = entry.file_name();
            let filename = filename.to_string_lossy();
            if !filename.starts_with("appmanifest_") || !filename.ends_with(".acf") {
                continue;
            }
            let bytes = match fs::read(entry.path()) {
                Ok(bytes) => bytes,
                Err(_) => {
                    diagnostics.push("could not read a Steam app manifest".to_owned());
                    continue;
                }
            };
            let game = match parse_app_manifest(&bytes) {
                Ok(game) => game,
                Err(error) => {
                    diagnostics.push(format!("ignored a Steam app manifest: {error}"));
                    continue;
                }
            };
            if !steamapps.join("common").join(&game.install_dir).is_dir() {
                diagnostics.push(
                    "ignored a Steam manifest whose install directory is unavailable".to_owned(),
                );
                continue;
            }
            if !games
                .iter()
                .any(|existing: &InstalledSteamGame| existing.app_id == game.app_id)
            {
                games.push(game);
            }
        }
    }
    games.sort_by_key(|game| game.app_id);
    SteamScan { games, diagnostics }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_manifest_without_using_nested_fields() {
        let manifest = br#"// Steam metadata
            "AppState" {
                "appid" "570"
                "name" "Dota 2"
                "installdir" "dota 2 beta"
                "MountedDepots" { "appid" "999" "nested" { "value" "ok" } }
            }"#;
        assert_eq!(
            parse_app_manifest(manifest),
            Ok(InstalledSteamGame {
                app_id: 570,
                name: "Dota 2".into(),
                install_dir: "dota 2 beta".into(),
            })
        );
    }

    #[test]
    fn diagnoses_missing_appid() {
        assert_eq!(
            parse_app_manifest(br#""AppState" { "name" "Game" "installdir" "Game" }"#),
            Err(ManifestDiagnostic::MissingField("appid"))
        );
    }

    #[test]
    fn requires_positive_decimal_u32_appid() {
        for id in ["", "0", "-1", "+1", "1.5", "abc", "4294967296"] {
            let manifest =
                format!(r#""AppState" {{ "appid" "{id}" "name" "Game" "installdir" "Game" }}"#);
            assert_eq!(
                parse_app_manifest(manifest.as_bytes()),
                Err(ManifestDiagnostic::InvalidField("appid"))
            );
        }
        let manifest = br#""AppState" { "appid" "4294967295" "name" "Game" "installdir" "Game" }"#;
        assert_eq!(
            parse_app_manifest(manifest).map(|game| game.app_id),
            Ok(u32::MAX)
        );
    }

    #[test]
    fn rejects_oversize_before_parsing() {
        assert_eq!(
            parse_app_manifest(&vec![b' '; MAX_MANIFEST_BYTES + 1]),
            Err(ManifestDiagnostic::InputTooLarge)
        );
        let mut manifest =
            br#""AppState" { "appid" "1" "name" "Game" "installdir" "Game" }"#.to_vec();
        manifest.resize(MAX_MANIFEST_BYTES, b' ');
        assert_eq!(parse_app_manifest(&manifest).map(|game| game.app_id), Ok(1));
    }

    #[test]
    fn rejects_malformed_quotes_objects_and_trailing_data() {
        for manifest in [
            r#""AppState" { "appid" "1 }"#,
            r#""AppState" { "appid" "1" "name" "Game" "installdir" "Game" "#,
            r#""AppState" { "appid" "1" "name" "Game" "installdir" "Game" } }"#,
            r#""AppState" { appid "1" }"#,
            r#""AppState" { "appid" "1" "name" "Game" "installdir" "Game" "extra" { "key" } }"#,
        ] {
            assert!(matches!(
                parse_app_manifest(manifest.as_bytes()),
                Err(ManifestDiagnostic::Malformed { .. })
            ));
        }
    }

    #[test]
    fn rejects_ambiguous_appid_and_unsafe_directory() {
        assert_eq!(
            parse_app_manifest(br#""AppState" { "appid" "1" "APPID" "2" }"#),
            Err(ManifestDiagnostic::DuplicateField("appid"))
        );
        for path in ["..", "../Game", "/Game", r"C:\Game", r"..\Game"] {
            let manifest =
                format!(r#""AppState" {{ "appid" "1" "name" "Game" "installdir" "{path}" }}"#);
            assert_eq!(
                parse_app_manifest(manifest.as_bytes()),
                Err(ManifestDiagnostic::InvalidField("installdir"))
            );
        }
    }

    #[test]
    fn decodes_escaped_quotes_and_preserves_unicode() {
        let manifest = r#""AppState" { "appid" "1" "name" "日本語 \"Game\"" "installdir" "Game" }"#;
        assert_eq!(
            parse_app_manifest(manifest.as_bytes()).map(|game| game.name),
            Ok("日本語 \"Game\"".into())
        );
    }

    #[test]
    fn diagnoses_invalid_utf8_and_excessive_nesting() {
        assert_eq!(
            parse_app_manifest(&[0xff]),
            Err(ManifestDiagnostic::InvalidUtf8)
        );
        let manifest = format!(
            "\"AppState\" {{ {}{} }}",
            "\"nested\" {".repeat(MAX_OBJECT_DEPTH),
            "}".repeat(MAX_OBJECT_DEPTH)
        );
        assert_eq!(
            parse_app_manifest(manifest.as_bytes()),
            Err(ManifestDiagnostic::NestingTooDeep)
        );
    }
}
