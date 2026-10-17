#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MatchResult {
    Matched(u32),
    Ambiguous,
    Unresolved,
}

pub(crate) fn normalized_title(title: &str) -> String {
    let lower = title.to_lowercase();
    let mut end = lower.len();
    for (index, _) in lower.char_indices() {
        let tail = &lower[index..];
        let bracket_label = tail
            .strip_prefix(" [")
            .and_then(|label| label.split(']').next());
        let release_label = bracket_label.is_some_and(|label| significant_tokens(label).is_empty());
        if release_label
            || tail.starts_with(" (v") && tail.as_bytes().get(3).is_some_and(u8::is_ascii_digit)
            || tail.starts_with(" v") && tail.as_bytes().get(2).is_some_and(u8::is_ascii_digit)
            || tail.starts_with(" build ")
            || tail.starts_with(" + dlc")
        {
            end = index;
            break;
        }
    }
    lower[..end]
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn significant_tokens(name: &str) -> Vec<&str> {
    name.split_whitespace()
        .filter(|word| {
            word.chars().any(char::is_numeric)
                || matches!(
                    *word,
                    "remastered"
                        | "remake"
                        | "deluxe"
                        | "definitive"
                        | "ultimate"
                        | "goty"
                        | "vr"
                        | "edition"
                )
        })
        .collect()
}

fn similarity(a: &str, b: &str) -> f64 {
    let a: Vec<_> = a.chars().collect();
    let b: Vec<_> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ac) in a.iter().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, bc) in b.iter().enumerate() {
            let old = row[j + 1];
            row[j + 1] = (previous + usize::from(ac != bc))
                .min(row[j] + 1)
                .min(old + 1);
            previous = old;
        }
    }
    1.0 - row[b.len()] as f64 / a.len().max(b.len()).max(1) as f64
}

pub(crate) fn match_title(title: &str, catalog: &[(u32, String)]) -> MatchResult {
    let title = normalized_title(title);
    if title.is_empty() || title.len() > 512 {
        return MatchResult::Unresolved;
    }
    let mut ranked: Vec<_> = catalog
        .iter()
        .filter_map(|(id, name)| {
            let name = normalized_title(name);
            if significant_tokens(&name) != significant_tokens(&title) {
                return None;
            }
            let score = similarity(&title, &name);
            (score >= 0.85).then_some((*id, score))
        })
        .collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let Some((id, score)) = ranked.first() else {
        return MatchResult::Unresolved;
    };
    if ranked.get(1).is_some_and(|(_, next)| score - next < 0.10) {
        return MatchResult::Ambiguous;
    }
    if *score < 0.95 {
        return MatchResult::Unresolved;
    }
    MatchResult::Matched(*id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_release_suffixes_without_confusing_sequels_or_editions() {
        let catalog = vec![(400, "Portal".to_owned()), (620, "Portal 2".to_owned())];
        assert_eq!(
            match_title("Portal v1.0 [SteamGG]", &catalog),
            MatchResult::Matched(400)
        );
        assert_eq!(
            match_title("Portal 2 (v2.1 + DLCs)", &catalog),
            MatchResult::Matched(620)
        );
        assert_eq!(
            match_title("Portal 3 v1.0", &catalog),
            MatchResult::Unresolved
        );
        assert_eq!(
            match_title("Portal [Remastered]", &catalog),
            MatchResult::Unresolved
        );
        assert_eq!(
            match_title("Portal Remastered", &catalog),
            MatchResult::Unresolved
        );
    }

    #[test]
    fn refuses_ambiguous_names_and_preserves_title_numbers() {
        let catalog = vec![
            (1, "Game".to_owned()),
            (2, "Game".to_owned()),
            (3, "F1 2026".to_owned()),
        ];
        assert_eq!(match_title("Game", &catalog), MatchResult::Ambiguous);
        assert_eq!(
            match_title("F1 2026 v1.2", &catalog),
            MatchResult::Matched(3)
        );
        assert_eq!(match_title("F1 2025", &catalog), MatchResult::Unresolved);
    }
}
