#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MatchResult {
    Matched(u32),
    Ambiguous,
    Unresolved,
}

fn folded_title(title: &str) -> String {
    let mut result = String::new();
    for c in title.chars().flat_map(char::to_lowercase) {
        match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ấ' | 'ắ' | 'ẳ' | 'ẵ' | 'ặ' | 'ầ' | 'ằ' | 'ȃ'
            | 'ả' | 'ạ' | 'ẩ' | 'ẫ' | 'ậ' | 'ā' | 'ă' | 'ą' | 'ǎ' | 'ǻ' | 'ȁ' => {
                result.push('a')
            }
            'æ' | 'ǽ' => result.push_str("ae"),
            'ç' | 'ḉ' | 'ć' | 'ĉ' | 'ċ' | 'č' => result.push('c'),
            'è' | 'é' | 'ê' | 'ë' | 'ế' | 'ḗ' | 'ề' | 'ḕ' | 'ḝ' | 'ȇ' | 'ẻ' | 'ẽ' | 'ẹ' | 'ể'
            | 'ễ' | 'ệ' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' | 'ȅ' | 'ȩ' => result.push('e'),
            'ì' | 'í' | 'î' | 'ï' | 'ḯ' | 'ȋ' | 'ỉ' | 'ị' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' | 'ǐ'
            | 'ȉ' => result.push('i'),
            'ð' | 'ď' | 'đ' | 'ḑ' => result.push('d'),
            'ñ' | 'ń' | 'ņ' | 'ň' | 'ŉ' | 'ǹ' => result.push('n'),
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ố' | 'ṍ' | 'ṓ' | 'ȏ' | 'ỏ' | 'ọ' | 'ổ' | 'ỗ'
            | 'ộ' | 'ờ' | 'ở' | 'ỡ' | 'ớ' | 'ợ' | 'ō' | 'ŏ' | 'ő' | 'ơ' | 'ǒ' | 'ǿ' | 'ồ' | 'ṑ'
            | 'ȍ' => result.push('o'),
            'ù' | 'ú' | 'û' | 'ü' | 'ủ' | 'ụ' | 'ử' | 'ữ' | 'ự' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű'
            | 'ų' | 'ȗ' | 'ư' | 'ǔ' | 'ǖ' | 'ǘ' | 'ǚ' | 'ǜ' | 'ứ' | 'ṹ' | 'ừ' | 'ȕ' => {
                result.push('u')
            }
            'ý' | 'ÿ' | 'ŷ' | 'ỳ' => result.push('y'),
            'ĝ' | 'ǵ' | 'ğ' | 'ġ' | 'ģ' | 'ǧ' => result.push('g'),
            'ĥ' | 'ħ' | 'ḫ' | 'ȟ' | 'ḩ' => result.push('h'),
            'ĳ' => result.push_str("ij"),
            'ĵ' | 'ǰ' => result.push('j'),
            'ķ' | 'ḱ' | 'ǩ' => result.push('k'),
            'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => result.push('l'),
            'ḿ' => result.push('m'),
            'œ' => result.push_str("oe"),
            'ŕ' | 'ŗ' | 'ř' | 'ȓ' | 'ȑ' => result.push('r'),
            'ś' | 'ŝ' | 'ş' | 'ș' | 'š' | 'ſ' | 'ṥ' | 'ṧ' => result.push('s'),
            'ţ' | 'ț' | 'ť' | 'ŧ' => result.push('t'),
            'ŵ' | 'ẃ' | 'ẁ' => result.push('w'),
            'ź' | 'ż' | 'ž' => result.push('z'),
            'ƒ' => result.push('f'),
            'þ' => result.push_str("th"),
            'ṕ' => result.push('p'),
            'ѓ' => result.push('г'),
            'ќ' => result.push('к'),
            'й' => result.push('и'),
            'ё' => result.push('е'),
            '\u{0300}'..='\u{036f}' => {}
            c => result.push(c),
        }
    }
    result
}

fn build_number(value: &str) -> bool {
    value
        .trim_start_matches(|c: char| {
            c.is_whitespace() || matches!(c, '-' | ':' | '.' | '\u{2013}' | '\u{2014}')
        })
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_digit)
}

fn release_metadata(label: &str) -> bool {
    let label = label.trim();
    let explicit_version = label
        .strip_prefix('v')
        .map(|value| value.trim_start_matches(['v', '.', '-', ' ']));
    let starts_version = explicit_version
        .is_some_and(|value| value.as_bytes().first().is_some_and(u8::is_ascii_digit))
        || label.as_bytes().first().is_some_and(u8::is_ascii_digit)
            && (label.contains('.') || label.contains("/build"));
    let edition = label
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| matches!(word, "edition" | "remastered" | "remake" | "vr"));
    let embedded_build = !edition
        && label
            .match_indices("build")
            .any(|(index, _)| build_number(&label[index + 5..]));
    starts_version
        || embedded_build
        || label.contains("dlc")
            && !edition
            && (label.starts_with("incl ") || label.starts_with("all dlc"))
        || matches!(
            label,
            "steamgg"
                | "steamgg.net"
                | "early access"
                | "full release"
                | "final version"
                | "latest version"
                | "final"
                | "all dlcs"
                | "all dlc"
                | "dlcs"
                | "dlc"
                | "online"
                | "steam version"
        )
}

pub(crate) fn normalized_title(title: &str) -> String {
    let mut lower = folded_title(title);
    let mut replacements = Vec::new();
    for (index, ch) in lower.char_indices() {
        if !matches!(ch, '[' | '(') {
            continue;
        }
        let close = if ch == '[' { ']' } else { ')' };
        let Some(end) = lower[index + 1..].find(close) else {
            continue;
        };
        let label = &lower[index + 1..index + 1 + end];
        let edition = label.contains("edition");
        if edition && let Some(build) = label.strip_prefix("build") {
            let suffix = build
                .trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '-' | ':' | '.'));
            let suffix = suffix
                .trim_start_matches(|c: char| c.is_ascii_digit() || c == '.')
                .trim_start();
            if !suffix.is_empty() {
                replacements.push((index + 1, index + 1 + end, suffix.to_owned()));
            }
        }
    }
    for (start, end, value) in replacements.into_iter().rev() {
        lower.replace_range(start..end, &value);
    }
    for label in [
        " free download pc game",
        " free download pc",
        " free download",
        " direct download",
        " download free",
        " free game",
        " free downloaad",
        " free downlad",
        " free downlaod",
        " free downlod",
        " free dowload",
        " free downoad",
        " free downlooad",
        " free diwnload",
        " download",
    ] {
        if let Some(index) = lower.find(label) {
            let tail = lower[index + label.len()..].trim_start();
            if tail.is_empty() || tail.starts_with(['[', '(']) {
                lower.replace_range(index..index + label.len(), "");
                break;
            }
        }
    }
    let mut end = lower.len();
    for (index, ch) in lower.char_indices() {
        let tail = &lower[index..];
        if matches!(ch, '[' | '(') {
            let close = if ch == '[' { ']' } else { ')' };
            if let Some(label) = tail[1..].split(close).next()
                && release_metadata(label)
            {
                end = index;
                break;
            }
        }
        let boundary = index > 0
            && lower[..index]
                .chars()
                .next_back()
                .is_some_and(|c| !c.is_alphanumeric());
        let version = tail
            .strip_prefix('v')
            .map(|value| value.trim_start_matches(['.', '-', ' ']));
        let build = tail.strip_prefix("build");
        if boundary
            && (version.is_some_and(|v| v.as_bytes().first().is_some_and(u8::is_ascii_digit))
                || build.is_some_and(build_number))
            || tail.starts_with(" + dlc")
        {
            end = index;
            break;
        }
    }
    let mut name = lower[..end].trim();
    if let Some(cleaned) = name.strip_prefix("download ") {
        name = cleaned.trim_start();
    }
    name.chars()
        .filter(|c| !matches!(c, '\'' | '’' | '‘' | 'ʼ' | '™' | '®'))
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

pub(crate) fn text_candidate_matches(release: &str, game: &str) -> bool {
    if significant_tokens(release) != significant_tokens(game) {
        return false;
    }
    let release_words: std::collections::HashSet<_> = release.split_whitespace().collect();
    let game_words: std::collections::HashSet<_> = game.split_whitespace().collect();
    game_words.iter().all(|word| release_words.contains(word))
        && release_words.difference(&game_words).all(|word| {
            matches!(
                *word,
                "free"
                    | "download"
                    | "direct"
                    | "pc"
                    | "game"
                    | "repack"
                    | "steamgg"
                    | "net"
                    | "com"
                    | "latest"
                    | "version"
                    | "portable"
                    | "uncensored"
                    | "online"
            )
        })
}

pub(crate) fn match_title(title: &str, catalog: &[(u32, String)]) -> Result<MatchResult, String> {
    let matched = conservative_match(title, catalog);
    if matched != MatchResult::Unresolved {
        return Ok(matched);
    }
    crate::source_search::find_matches(&[title.to_owned()], catalog).map(|matches| matches[0])
}

fn conservative_match(title: &str, catalog: &[(u32, String)]) -> MatchResult {
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

    fn match_title(title: &str, catalog: &[(u32, String)]) -> MatchResult {
        super::match_title(title, catalog).unwrap()
    }

    #[test]
    fn text_search_matches_extra_release_words_and_reordered_tokens() {
        let catalog = vec![
            (1, "Human Resource Machine".into()),
            (2, "SIMULACRA".into()),
        ];
        assert_eq!(
            match_title("Human Resource Machine SteamGG Repack", &catalog),
            MatchResult::Matched(1)
        );
        assert_eq!(
            match_title("Machine Human Resource", &catalog),
            MatchResult::Matched(1)
        );
        assert_eq!(
            match_title("Free Download SIMULACRA", &catalog),
            MatchResult::Matched(2)
        );
    }

    #[test]
    fn text_search_refuses_title_subsets_and_ambiguous_catalogue_ids() {
        assert_eq!(
            match_title("Portal Stories Mel", &[(1, "Portal".into())]),
            MatchResult::Unresolved
        );
        assert_eq!(
            match_title("Portal 2 SteamGG Repack", &[(1, "Portal".into())]),
            MatchResult::Unresolved
        );
        assert_eq!(
            match_title("Portal VR SteamGG Repack", &[(1, "Portal".into())]),
            MatchResult::Unresolved
        );
        assert_eq!(
            match_title(
                "Machine Human Resource",
                &[
                    (1, "Human Resource Machine".into()),
                    (2, "Resource Machine Human".into())
                ]
            ),
            MatchResult::Ambiguous
        );
    }

    #[test]
    fn normalization_is_stable_for_nested_edition_builds() {
        for (title, expected) in [
            (
                "DOOM 3 Free Download (Build 10301625 BFG Edition)",
                "doom 3 bfg edition",
            ),
            (
                "Grand Theft Auto San Andreas Free Download [Build-16391096 Definitive Edition]",
                "grand theft auto san andreas definitive edition",
            ),
            (
                "FTL: Faster Than Light Free Download (Advanced Edition.v1.6.13)",
                "ftl faster than light advanced edition",
            ),
            (
                "This War of Mine Free Download [Complete Edition-v6.0.0+ALL DLCs]",
                "this war of mine complete edition",
            ),
            (
                "Scritchy Scratchy Free Download (Build\t22395563)",
                "scritchy scratchy",
            ),
        ] {
            let actual = normalized_title(title);
            assert_eq!(actual, expected, "{title}");
            assert_eq!(normalized_title(&actual), actual, "{title}");
        }
    }

    #[test]
    fn folds_hydra_latin_characters_without_discarding_other_scripts() {
        assert_eq!(normalized_title("Pokémon Æon Œuvre"), "pokemon aeon oeuvre");
        assert_eq!(normalized_title("Cafe\u{301}"), normalized_title("Café"));
        assert_eq!(normalized_title("東京"), "東京");
    }

    #[test]
    fn removes_hotfix_build_payloads_but_keeps_explicit_editions() {
        for (title, expected) in [
            (
                "Baldurs Gate 3 Free Download [Hotfix#36/Build-22517190]",
                "baldurs gate 3",
            ),
            ("Arma 3 Free Download [#003/Build-22679123]", "arma 3"),
            (
                "Baby Steps Free Download [Patch.7.1/Build-21168055]",
                "baby steps",
            ),
            (
                "CARRION Free Download [Deluxe-Edition/Build-9877347]",
                "carrion deluxe edition",
            ),
            ("AMID EVIL Free Download (Incl ALL DLCs)", "amid evil"),
            ("DeadPoly Free Download PC [Build-22657454]", "deadpoly"),
        ] {
            assert_eq!(normalized_title(title), expected, "{title}");
        }
    }

    #[test]
    fn removes_marketing_before_metadata_without_erasing_edition_labels() {
        for (title, expected) in [
            ("Among Us Free Download [v191+Multiplayer]", "among us"),
            (
                "Absolum Free Download [v-1.1/Build-21713036+Co-Op]",
                "absolum",
            ),
            (
                "ARK: Survival Evolved Free Download [v.356.20]",
                "ark survival evolved",
            ),
            ("Aground Zero Free Download (Early Access)", "aground zero"),
            (
                "Ark of Charon Free Download [Build.15434213]",
                "ark of charon",
            ),
            (
                "Ara: History Untold Free Download [Deluxe Edition v1.0.0.316-206104]",
                "ara history untold deluxe edition",
            ),
            ("Game Free Download [Remastered]", "game remastered"),
            ("Game Free Download [Campaign Only]", "game campaign only"),
        ] {
            assert_eq!(normalized_title(title), expected, "{title}");
        }
    }

    #[test]
    fn cleans_steamgg_marketing_builds_and_release_versions() {
        for (title, expected) in [
            (
                "Beyond These Stars Free Download [v1.0.2244/Build-25972788]",
                "beyond these stars",
            ),
            (
                "Devices Tycoon Free Download [Build-25997547]",
                "devices tycoon",
            ),
            (
                "Brick by Brick: Tidy Up the Small Town! Free Download (Build 25964658)",
                "brick by brick tidy up the small town",
            ),
            (
                "Disney/Pixar Toy Story 3 Complete Edition [Build-25668953]",
                "disney pixar toy story 3 complete edition",
            ),
            (
                "Cloudpunk Free Download (Build 8245592 + City of Ghosts DLC)",
                "cloudpunk",
            ),
            (
                "Invokyr Free Download [v0.44.11/Build 25957256+ONLINE]",
                "invokyr",
            ),
            ("Download Human Resource Machine", "human resource machine"),
            (
                "OCTOPATH TRAVELER Download (Build 5272616)",
                "octopath traveler",
            ),
            ("Returnal Download Free [1.0/Build 11083317]", "returnal"),
            (
                "DARK SOULS Prepare To Die Edition Free Downlad",
                "dark souls prepare to die edition",
            ),
            (
                "Tiny Terraces Free Diwnload (Build 23747267)",
                "tiny terraces",
            ),
            (
                "VEIN Direct Download [v0.024/Build-23899965+ ONLINE]",
                "vein",
            ),
        ] {
            assert_eq!(normalized_title(title), expected, "{title}");
        }
    }

    #[test]
    fn punctuation_does_not_split_apostrophes_or_trademarks() {
        assert_eq!(
            normalized_title("The Witch’s Bakery™"),
            normalized_title("The Witchs Bakery")
        );
        assert_eq!(normalized_title("S.T.A.L.K.E.R._2"), "s t a l k e r 2");
        assert_eq!(normalized_title("NieR:Automata™"), "nier automata");
    }

    #[test]
    fn preserves_real_title_words_and_edition_identity() {
        for title in [
            "Free Download Simulator",
            "Download",
            "Build 2026",
            "Version 2",
            "V Rising",
            "Portal [Remastered]",
            "Game (VR)",
        ] {
            assert!(!normalized_title(title).is_empty(), "{title}");
        }
        assert_eq!(
            normalized_title("Free Download Simulator"),
            "free download simulator"
        );
        let catalog = vec![(1, "Portal".into()), (2, "Portal 2".into())];
        assert_eq!(
            match_title("Portal 2 Free Download [Build-12345]", &catalog),
            MatchResult::Matched(2)
        );
        assert_eq!(
            match_title("Portal 3 Free Download [v1.0]", &catalog),
            MatchResult::Unresolved
        );
        assert_eq!(
            match_title("Portal Remastered Free Download [v1.0]", &catalog),
            MatchResult::Unresolved
        );
    }

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
