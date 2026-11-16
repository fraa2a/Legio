//! Tolerant extractor of scalar values from untrusted YAML written by other
//! launchers (Lutris game files, Bottles `bottle.yml`).
//!
//! It is not a YAML parser. It follows `key: value` lines by indentation and
//! gives up on a value (returns `None`) whenever the syntax is uncertain:
//! anchors, aliases, tags, flow collections, block scalars, multi-line
//! scalars, escapes in double quotes, repeated keys. This avoids alias
//! expansion and keeps the extraction bounded and predictable.

const MAX_LINES: usize = 20_000;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Value {
    Empty,
    Scalar(String),
    Uncertain,
}

#[derive(Debug)]
struct Entry {
    indent: usize,
    path: Vec<String>,
    value: Value,
}

#[derive(Debug, Default)]
pub struct YamlScalars {
    entries: Vec<Entry>,
}

impl YamlScalars {
    pub fn parse(text: &str) -> Self {
        let mut entries: Vec<Entry> = Vec::new();
        let mut stack: Vec<(usize, String)> = Vec::new();
        for line in text.lines().take(MAX_LINES) {
            let trimmed = line.trim_start_matches(' ');
            if line.starts_with('\t') || trimmed.starts_with('\t') {
                return Self::default();
            }
            if trimmed.trim().is_empty()
                || trimmed.starts_with('#')
                || trimmed.starts_with("---")
                || trimmed.starts_with("...")
            {
                continue;
            }
            let indent = line.len() - trimmed.len();
            let Some((key, rest)) = split_key(trimmed) else {
                // A deeper non-key line continues the previous plain scalar.
                if let Some(last) = entries.last_mut()
                    && indent > last.indent
                    && matches!(last.value, Value::Scalar(_))
                {
                    last.value = Value::Uncertain;
                }
                continue;
            };
            while stack.last().is_some_and(|(level, _)| *level >= indent) {
                stack.pop();
            }
            stack.push((indent, key));
            entries.push(Entry {
                indent,
                path: stack.iter().map(|(_, key)| key.clone()).collect(),
                value: parse_value(rest),
            });
        }
        Self { entries }
    }

    /// The scalar at `path`, only when it appears exactly once and is certain.
    pub fn scalar(&self, path: &[&str]) -> Option<String> {
        let mut matches = self.entries.iter().filter(|entry| entry.path == path);
        let entry = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        match &entry.value {
            Value::Scalar(value) => Some(value.clone()),
            _ => None,
        }
    }

    /// Distinct keys directly under `path`, in file order.
    pub fn children(&self, path: &[&str]) -> Vec<String> {
        let mut keys: Vec<String> = Vec::new();
        for entry in &self.entries {
            if entry.path.len() == path.len() + 1 && entry.path[..path.len()] == *path {
                let key = &entry.path[path.len()];
                if !keys.contains(key) {
                    keys.push(key.clone());
                }
            }
        }
        keys
    }
}

fn split_key(line: &str) -> Option<(String, &str)> {
    if line.starts_with("- ") || line == "-" {
        return None;
    }
    let first = line.chars().next()?;
    if matches!(first, '"' | '\'') {
        let end = line[1..].find(first)? + 1;
        let rest = line[end + 1..].strip_prefix(':')?;
        if !rest.is_empty() && !rest.starts_with(' ') {
            return None;
        }
        return Some((line[1..end].to_owned(), rest));
    }
    if matches!(
        first,
        '{' | '[' | '&' | '*' | '!' | '|' | '>' | '%' | '@' | '?' | '#'
    ) {
        return None;
    }
    let mut search = 0;
    while let Some(position) = line[search..].find(':') {
        let at = search + position;
        let rest = &line[at + 1..];
        if rest.is_empty() || rest.starts_with(' ') {
            let key = line[..at].trim();
            return (!key.is_empty()).then(|| (key.to_owned(), rest));
        }
        search = at + 1;
    }
    None
}

fn is_comment_or_empty(text: &str) -> bool {
    let text = text.trim();
    text.is_empty() || text.starts_with('#')
}

fn parse_value(raw: &str) -> Value {
    let raw = raw.trim();
    let Some(first) = raw.chars().next() else {
        return Value::Empty;
    };
    match first {
        '#' => Value::Empty,
        '|' | '>' | '&' | '*' | '!' | '{' | '[' | '%' | '@' | '`' => Value::Uncertain,
        '\'' => {
            let mut value = String::new();
            let mut chars = raw[1..].char_indices().peekable();
            while let Some((index, character)) = chars.next() {
                if character == '\'' {
                    if chars.peek().is_some_and(|(_, next)| *next == '\'') {
                        value.push('\'');
                        chars.next();
                    } else if is_comment_or_empty(&raw[index + 2..]) {
                        return Value::Scalar(value);
                    } else {
                        return Value::Uncertain;
                    }
                } else {
                    value.push(character);
                }
            }
            Value::Uncertain
        }
        '"' => match raw[1..].find('"') {
            Some(end) => {
                let inner = &raw[1..=end];
                if inner.contains('\\') || !is_comment_or_empty(&raw[end + 2..]) {
                    Value::Uncertain
                } else {
                    Value::Scalar(inner.to_owned())
                }
            }
            None => Value::Uncertain,
        },
        _ => {
            let plain = match raw.find(" #") {
                Some(at) => raw[..at].trim_end(),
                None => raw,
            };
            if matches!(plain, "~" | "null" | "Null" | "NULL") {
                Value::Empty
            } else {
                Value::Scalar(plain.to_owned())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_nested_scalars() {
        let yaml = "# comment\ngame:\n  exe: /games/My Game/game.exe # trailing\n  prefix: '/home/u/it''s'\n  args: \"-windowed\"\nwine:\n  version: lutris-7.2\n";
        let parsed = YamlScalars::parse(yaml);
        assert_eq!(
            parsed.scalar(&["game", "exe"]).as_deref(),
            Some("/games/My Game/game.exe")
        );
        assert_eq!(
            parsed.scalar(&["game", "prefix"]).as_deref(),
            Some("/home/u/it's")
        );
        assert_eq!(
            parsed.scalar(&["game", "args"]).as_deref(),
            Some("-windowed")
        );
        assert_eq!(
            parsed.scalar(&["wine", "version"]).as_deref(),
            Some("lutris-7.2")
        );
        assert_eq!(parsed.scalar(&["game", "missing"]), None);
        assert_eq!(parsed.children(&["game"]), ["exe", "prefix", "args"]);
    }

    #[test]
    fn skips_values_it_cannot_read_with_certainty() {
        let yaml = "game:\n  exe: &anchor /a\n  prefix: *anchor\n  a: !!str x\n  b: {x: 1}\n  c: |\n    text\n  d: \"C:\\\\win\"\n  e: first\n    continued\n  f: 'open\n";
        let parsed = YamlScalars::parse(yaml);
        for key in ["exe", "prefix", "a", "b", "c", "d", "e", "f"] {
            assert_eq!(parsed.scalar(&["game", key]), None, "{key}");
        }
    }

    #[test]
    fn rejects_repeated_keys_tabs_and_unrelated_content() {
        let parsed = YamlScalars::parse("game:\n  exe: /a\ngame:\n  exe: /b\n");
        assert_eq!(parsed.scalar(&["game", "exe"]), None);
        assert_eq!(
            YamlScalars::parse("game:\n\texe: /a\n").scalar(&["game", "exe"]),
            None
        );
        let parsed = YamlScalars::parse("\u{0}\u{1}binary garbage\n- item\n: :\n");
        assert!(parsed.children(&[]).is_empty());
        let parsed = YamlScalars::parse("game:\n  args:\n    - a\n    - b\n  exe: /x\n");
        assert_eq!(parsed.scalar(&["game", "exe"]).as_deref(), Some("/x"));
        assert_eq!(parsed.scalar(&["game", "args"]), None);
    }

    #[test]
    fn colons_inside_values_and_keys_are_kept() {
        let parsed = YamlScalars::parse(
            "External_Programs:\n  id-1:\n    path: C:\\Games\\a.exe\n    name: A: B\n",
        );
        assert_eq!(
            parsed
                .scalar(&["External_Programs", "id-1", "path"])
                .as_deref(),
            Some("C:\\Games\\a.exe")
        );
        assert_eq!(
            parsed
                .scalar(&["External_Programs", "id-1", "name"])
                .as_deref(),
            Some("A: B")
        );
    }

    #[test]
    fn bounds_the_number_of_lines() {
        let text = "a: 1\n".repeat(MAX_LINES + 10);
        assert!(YamlScalars::parse(&text).entries.len() <= MAX_LINES);
    }
}
