use std::collections::HashSet;
use std::fmt;

pub const MAX_LOGINUSERS_BYTES: usize = 1024 * 1024;
const MAX_DEPTH: usize = 32;
const MAX_ENTRIES: usize = 100_000;
const MAX_ACCOUNTS: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoginUser {
    pub steam_id: String,
    pub account_name: Option<String>,
    pub persona_name: Option<String>,
    pub remember_password: Option<String>,
    pub auto_login: Option<String>,
    pub most_recent: Option<String>,
    fields: Vec<FieldSpan>,
    close: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FieldSpan {
    name: String,
    value_start: usize,
    value_end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VdfError {
    InputTooLarge,
    InvalidUtf8,
    Malformed { offset: usize },
    NestingTooDeep,
    TooManyEntries,
    TooManyAccounts,
    InvalidSteamId,
    DuplicateField(&'static str),
    AccountNotFound,
    MissingAccountName,
    ScalarNotFound,
    AmbiguousSelection,
}

impl fmt::Display for VdfError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputTooLarge => formatter.write_str("loginusers.vdf exceeds the size limit"),
            Self::InvalidUtf8 => formatter.write_str("loginusers.vdf is not valid UTF-8"),
            Self::Malformed { offset } => {
                write!(formatter, "Malformed Steam KeyValues at byte {offset}")
            }
            Self::NestingTooDeep => formatter.write_str("Steam KeyValues nesting is too deep"),
            Self::TooManyEntries => formatter.write_str("loginusers.vdf has too many entries"),
            Self::TooManyAccounts => formatter.write_str("loginusers.vdf has too many accounts"),
            Self::InvalidSteamId => formatter.write_str("loginusers.vdf has an invalid Steam ID"),
            Self::DuplicateField(field) => write!(formatter, "loginusers.vdf repeats {field}"),
            Self::AccountNotFound => formatter.write_str("Steam account was not found"),
            Self::MissingAccountName => {
                formatter.write_str("Steam account has no saved account name")
            }
            Self::ScalarNotFound => formatter.write_str("Steam configuration field was not found"),
            Self::AmbiguousSelection => formatter.write_str("Steam account selection is ambiguous"),
        }
    }
}

impl std::error::Error for VdfError {}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    value: String,
    start: usize,
    end: usize,
    kind: TokenKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    Text,
    Open,
    Close,
}

struct Parser<'a> {
    bytes: &'a [u8],
    offset: usize,
    entries: usize,
}

pub(crate) fn parse_loginusers(bytes: &[u8]) -> Result<Vec<LoginUser>, VdfError> {
    if bytes.len() > MAX_LOGINUSERS_BYTES {
        return Err(VdfError::InputTooLarge);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| VdfError::InvalidUtf8)?;
    let mut parser = Parser {
        bytes,
        offset: usize::from(text.starts_with('\u{feff}')) * 3,
        entries: 0,
    };
    if !matches!(parser.next()?, Some(token) if token.kind == TokenKind::Text && token.value.eq_ignore_ascii_case("users"))
        || !matches!(parser.next()?, Some(token) if token.kind == TokenKind::Open)
    {
        return Err(parser.malformed());
    }
    let mut users = Vec::new();
    let mut ids = HashSet::new();
    loop {
        let Some(token) = parser.next()? else {
            return Err(parser.malformed());
        };
        if token.kind == TokenKind::Close {
            break;
        }
        if token.kind != TokenKind::Text || !valid_steam_id(&token.value) {
            return Err(VdfError::InvalidSteamId);
        }
        if !ids.insert(token.value.clone()) {
            return Err(VdfError::DuplicateField("Steam ID"));
        }
        if !matches!(parser.next()?, Some(open) if open.kind == TokenKind::Open) {
            return Err(parser.malformed());
        }
        let user = parser.user(token.value)?;
        users.push(user);
        if users.len() > MAX_ACCOUNTS {
            return Err(VdfError::TooManyAccounts);
        }
    }
    if parser.next()?.is_some() {
        return Err(parser.malformed());
    }
    Ok(users)
}

/// Returns a remembered selection only when AutoLogin and MostRecent agree uniquely.
/// This is persisted startup intent, not proof of the account currently online.
pub(crate) fn selected_account_id(bytes: &[u8]) -> Result<Option<String>, VdfError> {
    let users = parse_loginusers(bytes)?;
    let selected: Vec<_> = users
        .iter()
        .filter(|user| is_one(user.auto_login.as_deref()) && is_one(user.most_recent.as_deref()))
        .collect();
    match selected.as_slice() {
        [] => Ok(None),
        [user] => Ok(Some(user.steam_id.clone())),
        _ => Err(VdfError::AmbiguousSelection),
    }
}

/// Changes only the selected account flags and inserts missing required fields.
pub(crate) fn patch_account_selection(
    bytes: &[u8],
    target_steam_id: &str,
) -> Result<Vec<u8>, VdfError> {
    if !valid_steam_id(target_steam_id) {
        return Err(VdfError::InvalidSteamId);
    }
    let users = parse_loginusers(bytes)?;
    let target = users
        .iter()
        .find(|user| user.steam_id == target_steam_id)
        .ok_or(VdfError::AccountNotFound)?;
    if target.account_name.as_deref().is_none_or(str::is_empty) {
        return Err(VdfError::MissingAccountName);
    }
    let mut edits = Vec::new();
    for user in &users {
        let enabled = user.steam_id == target_steam_id;
        let value = if enabled { "1" } else { "0" };
        let mut set_flag = |name: &str, current: Option<&str>, include_if_missing: bool| {
            if let Some(field) = user
                .fields
                .iter()
                .find(|field| field.name.eq_ignore_ascii_case(name))
            {
                if current != Some(value) {
                    edits.push((field.value_start, field.value_end, format!("\"{value}\"")));
                }
            } else if include_if_missing {
                edits.push((
                    user.close,
                    user.close,
                    format!("\n\t\t\"{name}\" \"{value}\""),
                ));
            }
        };
        set_flag("AutoLogin", user.auto_login.as_deref(), true);
        set_flag("MostRecent", user.most_recent.as_deref(), true);
        if enabled {
            set_flag("RememberPassword", user.remember_password.as_deref(), true);
        }
    }
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.0));
    let mut output = bytes.to_vec();
    for (start, end, replacement) in edits {
        output.splice(start..end, replacement.bytes());
    }
    Ok(output)
}

/// Replaces one existing scalar addressed by its nested KeyValues path.
/// Every object in the document is parsed before returning a modified copy.
pub(crate) fn patch_keyvalues_scalar(
    bytes: &[u8],
    path: &[&str],
    value: &str,
) -> Result<Vec<u8>, VdfError> {
    if bytes.len() > MAX_LOGINUSERS_BYTES {
        return Err(VdfError::InputTooLarge);
    }
    if path.is_empty() || value.chars().any(char::is_control) {
        return Err(VdfError::Malformed { offset: 0 });
    }
    let text = std::str::from_utf8(bytes).map_err(|_| VdfError::InvalidUtf8)?;
    let mut parser = Parser {
        bytes,
        offset: usize::from(text.starts_with('\u{feff}')) * 3,
        entries: 0,
    };
    if !matches!(parser.next()?, Some(token) if token.kind == TokenKind::Text)
        || !matches!(parser.next()?, Some(token) if token.kind == TokenKind::Open)
    {
        return Err(parser.malformed());
    }
    let mut matches = Vec::new();
    parser.find_path(1, path, &mut matches)?;
    if parser.next()?.is_some() {
        return Err(parser.malformed());
    }
    let [(start, end)] = matches.as_slice() else {
        return Err(if matches.is_empty() {
            VdfError::ScalarNotFound
        } else {
            VdfError::AmbiguousSelection
        });
    };
    let replacement = format!("\"{}\"", escape_scalar(value));
    let mut output = bytes.to_vec();
    output.splice(*start..*end, replacement.bytes());
    Ok(output)
}

fn escape_scalar(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped
}

fn valid_steam_id(value: &str) -> bool {
    value
        .parse::<u64>()
        .ok()
        .is_some_and(|id| id > 0 && id.to_string() == value)
}

fn is_one(value: Option<&str>) -> bool {
    value.is_some_and(|value| value == "1")
}

impl Parser<'_> {
    fn malformed(&self) -> VdfError {
        VdfError::Malformed {
            offset: self.offset,
        }
    }

    fn next(&mut self) -> Result<Option<Token>, VdfError> {
        loop {
            while self
                .bytes
                .get(self.offset)
                .is_some_and(u8::is_ascii_whitespace)
            {
                self.offset += 1;
            }
            if self.bytes.get(self.offset..self.offset + 2) != Some(b"//") {
                break;
            }
            while self
                .bytes
                .get(self.offset)
                .is_some_and(|byte| *byte != b'\n')
            {
                self.offset += 1;
            }
        }
        let start = self.offset;
        let Some(byte) = self.bytes.get(self.offset).copied() else {
            return Ok(None);
        };
        self.offset += 1;
        let kind = match byte {
            b'{' => TokenKind::Open,
            b'}' => TokenKind::Close,
            b'"' => return self.quoted(start).map(Some),
            _ => return Err(self.malformed()),
        };
        Ok(Some(Token {
            value: String::new(),
            start,
            end: self.offset,
            kind,
        }))
    }

    fn quoted(&mut self, start: usize) -> Result<Token, VdfError> {
        let content_start = self.offset;
        let mut decoded = String::new();
        let mut segment = content_start;
        while let Some(&byte) = self.bytes.get(self.offset) {
            match byte {
                b'"' => {
                    let content_end = self.offset;
                    let raw = std::str::from_utf8(&self.bytes[segment..content_end])
                        .map_err(|_| VdfError::InvalidUtf8)?;
                    decoded.push_str(raw);
                    self.offset += 1;
                    return Ok(Token {
                        value: decoded,
                        start,
                        end: self.offset,
                        kind: TokenKind::Text,
                    });
                }
                b'\\' => {
                    let next = self.bytes.get(self.offset + 1).copied();
                    if matches!(next, Some(b'"' | b'\\' | b'n' | b'r' | b't')) {
                        let raw = std::str::from_utf8(&self.bytes[segment..self.offset])
                            .map_err(|_| VdfError::InvalidUtf8)?;
                        decoded.push_str(raw);
                        let escaped = match next.unwrap_or_default() {
                            b'"' => '"',
                            b'\\' => '\\',
                            b'n' => '\n',
                            b'r' => '\r',
                            _ => '\t',
                        };
                        decoded.push(escaped);
                        self.offset += 2;
                        segment = self.offset;
                    } else if next.is_none() {
                        return Err(self.malformed());
                    } else {
                        self.offset += 1;
                    }
                }
                0..=31 => return Err(self.malformed()),
                _ => self.offset += 1,
            }
        }
        Err(self.malformed())
    }

    fn user(&mut self, steam_id: String) -> Result<LoginUser, VdfError> {
        let mut user = LoginUser {
            steam_id,
            account_name: None,
            persona_name: None,
            remember_password: None,
            auto_login: None,
            most_recent: None,
            fields: Vec::new(),
            close: 0,
        };
        loop {
            let key = match self.next()? {
                Some(token) if token.kind == TokenKind::Close => {
                    user.close = token.start;
                    return Ok(user);
                }
                Some(token) if token.kind == TokenKind::Text => token,
                _ => return Err(self.malformed()),
            };
            let value = self.next()?.ok_or_else(|| self.malformed())?;
            self.count_entry()?;
            match value.kind {
                TokenKind::Text => {
                    let field = key.value.as_str();
                    let slot = if field.eq_ignore_ascii_case("AccountName") {
                        Some((&mut user.account_name, "AccountName"))
                    } else if field.eq_ignore_ascii_case("PersonaName") {
                        Some((&mut user.persona_name, "PersonaName"))
                    } else if field.eq_ignore_ascii_case("RememberPassword") {
                        Some((&mut user.remember_password, "RememberPassword"))
                    } else if field.eq_ignore_ascii_case("AutoLogin") {
                        Some((&mut user.auto_login, "AutoLogin"))
                    } else if field.eq_ignore_ascii_case("MostRecent") {
                        Some((&mut user.most_recent, "MostRecent"))
                    } else {
                        None
                    };
                    if let Some((slot, label)) = slot {
                        if slot.is_some() {
                            return Err(VdfError::DuplicateField(label));
                        }
                        *slot = Some(value.value.clone());
                        user.fields.push(FieldSpan {
                            name: key.value,
                            value_start: value.start,
                            value_end: value.end,
                        });
                    }
                }
                TokenKind::Open => self.skip_object(2)?,
                TokenKind::Close => return Err(self.malformed()),
            }
        }
    }

    fn skip_object(&mut self, depth: usize) -> Result<(), VdfError> {
        if depth > MAX_DEPTH {
            return Err(VdfError::NestingTooDeep);
        }
        loop {
            let Some(key) = self.next()? else {
                return Err(self.malformed());
            };
            if key.kind == TokenKind::Close {
                return Ok(());
            }
            if key.kind != TokenKind::Text {
                return Err(self.malformed());
            }
            let value = self.next()?.ok_or_else(|| self.malformed())?;
            self.count_entry()?;
            match value.kind {
                TokenKind::Text => {}
                TokenKind::Open => self.skip_object(depth + 1)?,
                TokenKind::Close => return Err(self.malformed()),
            }
        }
    }

    fn find_path(
        &mut self,
        depth: usize,
        path: &[&str],
        matches: &mut Vec<(usize, usize)>,
    ) -> Result<(), VdfError> {
        if depth > MAX_DEPTH {
            return Err(VdfError::NestingTooDeep);
        }
        loop {
            let Some(key) = self.next()? else {
                return Err(self.malformed());
            };
            if key.kind == TokenKind::Close {
                return Ok(());
            }
            if key.kind != TokenKind::Text {
                return Err(self.malformed());
            }
            let value = self.next()?.ok_or_else(|| self.malformed())?;
            self.count_entry()?;
            let matching_key = key.value.eq_ignore_ascii_case(path[0]);
            match value.kind {
                TokenKind::Text if matching_key && path.len() == 1 => {
                    matches.push((value.start, value.end));
                }
                TokenKind::Open if matching_key && path.len() > 1 => {
                    self.find_path(depth + 1, &path[1..], matches)?;
                }
                TokenKind::Text => {}
                TokenKind::Open => self.skip_object(depth + 1)?,
                TokenKind::Close => return Err(self.malformed()),
            }
        }
    }

    fn count_entry(&mut self) -> Result<(), VdfError> {
        self.entries += 1;
        if self.entries > MAX_ENTRIES {
            return Err(VdfError::TooManyEntries);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "// header\n\"users\" {\n  \"76561198000000001\" {\n    \"AccountName\" \"login-one\"\n    \"PersonaName\" \"Same name\"\n    \"RememberPassword\" \"1\"\n    \"AutoLogin\" \"0\"\n    \"MostRecent\" \"0\"\n    \"AllowAutoLogin\" \"1\"\n    \"FutureField\" { \"nested\" \"preserve me\" }\n  }\n  \"76561198000000002\" {\n    \"AccountName\" \"login-two\"\n    \"RememberPassword\" \"1\"\n    \"AutoLogin\" \"1\"\n    \"MostRecent\" \"1\"\n    \"AllowAutoLogin\" \"1\"\n    \"Unknown\" \"untouched\"\n  }\n}\n";

    #[test]
    fn patch_changes_only_account_flags_and_preserves_unknown_bytes() {
        let patched = patch_account_selection(SAMPLE.as_bytes(), "76561198000000001").unwrap();
        let text = std::str::from_utf8(&patched).unwrap();
        assert!(text.contains("// header\n"));
        assert!(text.contains("\"FutureField\" { \"nested\" \"preserve me\" }"));
        assert!(text.contains("\"Unknown\" \"untouched\""));
        let users = parse_loginusers(&patched).unwrap();
        assert_eq!(users[0].auto_login.as_deref(), Some("1"));
        assert_eq!(users[0].most_recent.as_deref(), Some("1"));
        assert_eq!(users[1].auto_login.as_deref(), Some("0"));
        assert_eq!(users[1].most_recent.as_deref(), Some("0"));
        assert!(
            std::str::from_utf8(&patched)
                .unwrap()
                .contains("\"AllowAutoLogin\" \"1\"")
        );
        assert_eq!(
            selected_account_id(&patched).unwrap().as_deref(),
            Some("76561198000000001")
        );
    }

    #[test]
    fn patch_resolves_target_and_requires_saved_account_name_and_password_flag() {
        assert_eq!(
            patch_account_selection(SAMPLE.as_bytes(), "76561198000000003"),
            Err(VdfError::AccountNotFound)
        );
        let no_name = SAMPLE.replace(
            "\"AccountName\" \"login-one\"",
            "\"OtherName\" \"login-one\"",
        );
        assert_eq!(
            patch_account_selection(no_name.as_bytes(), "76561198000000001"),
            Err(VdfError::MissingAccountName)
        );
        let disabled_password =
            SAMPLE.replace("\"RememberPassword\" \"1\"", "\"RememberPassword\" \"0\"");
        let patched =
            patch_account_selection(disabled_password.as_bytes(), "76561198000000001").unwrap();
        assert!(
            std::str::from_utf8(&patched)
                .unwrap()
                .contains("\"RememberPassword\" \"1\"")
        );
    }

    #[test]
    fn patch_inserts_missing_required_flags_and_leaves_other_fields_alone() {
        let input = br#""users" { "76561198000000001" { "AccountName" "login" "RememberPassword" "1" "Unknown" "value" } }"#;
        let patched = patch_account_selection(input, "76561198000000001").unwrap();
        let text = std::str::from_utf8(&patched).unwrap();
        assert!(text.contains("\"AutoLogin\" \"1\""));
        assert!(text.contains("\"MostRecent\" \"1\""));
        assert!(text.contains("\"Unknown\" \"value\""));
    }

    #[test]
    fn selected_id_requires_one_account_with_both_selection_flags() {
        assert_eq!(
            selected_account_id(SAMPLE.as_bytes()).unwrap(),
            Some("76561198000000002".to_owned())
        );
        let ambiguous = SAMPLE
            .replace("\"AutoLogin\" \"0\"", "\"AutoLogin\" \"1\"")
            .replace("\"MostRecent\" \"0\"", "\"MostRecent\" \"1\"");
        assert_eq!(
            selected_account_id(ambiguous.as_bytes()),
            Err(VdfError::AmbiguousSelection)
        );
        let recent_only = SAMPLE.replace("\"AutoLogin\" \"1\"", "\"AutoLogin\" \"0\"");
        assert_eq!(selected_account_id(recent_only.as_bytes()).unwrap(), None);
    }

    #[test]
    fn parser_enforces_size_depth_and_structure_limits() {
        assert_eq!(
            parse_loginusers(&vec![b' '; MAX_LOGINUSERS_BYTES + 1]),
            Err(VdfError::InputTooLarge)
        );
        assert!(parse_loginusers(br#""users" { "01" { } }"#).is_err());
        let nested = format!(
            "\"users\" {{ \"1\" {{ \"Unknown\" {{ {} }} }} }}",
            "\"k\" { ".repeat(MAX_DEPTH) + "\"v\" \"x\"" + &" }".repeat(MAX_DEPTH + 1)
        );
        assert_eq!(
            parse_loginusers(nested.as_bytes()),
            Err(VdfError::NestingTooDeep)
        );
    }

    #[test]
    fn scalar_patch_preserves_unrelated_configuration_and_validates_unique_path() {
        let input = br#"// keep
"InstallConfigStore" {
  "Software" {
    "Valve" {
      "Steam" {
        "AutoLoginUser" "old-login"
        "Other" "preserve"
      }
    }
  }
}"#;
        let patched = patch_keyvalues_scalar(
            input,
            &["Software", "Valve", "Steam", "AutoLoginUser"],
            "new\"login",
        )
        .unwrap();
        let text = std::str::from_utf8(&patched).unwrap();
        assert!(text.starts_with("// keep\n"));
        assert!(text.contains("\"AutoLoginUser\" \"new\\\"login\""));
        assert!(text.contains("\"Other\" \"preserve\""));
        assert_eq!(
            patch_keyvalues_scalar(input, &["Missing"], "value"),
            Err(VdfError::ScalarNotFound)
        );
        let duplicate = br#""root" { "nested" { "target" "one" } "nested" { "target" "two" } }"#;
        assert_eq!(
            patch_keyvalues_scalar(duplicate, &["nested", "target"], "value"),
            Err(VdfError::AmbiguousSelection)
        );
        let malformed = br#""root" { "safe" "value" "broken" { "unterminated" "value" }"#;
        assert!(patch_keyvalues_scalar(malformed, &["safe"], "new").is_err());
    }
}
