//! Recognizes the files of a multipart archive by name. Matching is strict and ignores
//! ASCII case, so look-alike names such as `game.part.rar` or `game.7z.01` are not parts.

use serde::{Deserialize, Serialize};
use std::fmt;

/// More parts than a GoFile folder may list, so a real set never reaches it.
const MAX_PARTS: usize = 999;

/// How the volumes of a set are named and how they join.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PartScheme {
    /// `game.part1.rar`, `game.part2.rar` (any zero padding).
    RarVolumes,
    /// `game.rar`, `game.r00`, `game.r01`.
    RarLegacyVolumes,
    /// `game.7z.001`, `game.7z.002`: a plain byte split of one 7z archive.
    SevenZipSplit,
    /// `game.zip.001`, `game.zip.002`: a plain byte split of one ZIP archive.
    ZipSplit,
}

/// A complete set: `order` lists the indexes of the input names in volume order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PartSet {
    pub scheme: PartScheme,
    pub order: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PartSetError {
    Missing(String),
    Duplicate(String),
    Unexpected(String),
    Mixed(String, String),
    /// `game.z01` + `game.zip` store offsets per volume and cannot be read as one stream.
    SpannedZip,
    UnsupportedSplit(String),
    TooMany,
}

impl fmt::Display for PartSetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(name) => {
                write!(formatter, "The multipart archive is incomplete: {name} is missing")
            }
            Self::Duplicate(name) => {
                write!(formatter, "The multipart archive lists {name} more than once")
            }
            Self::Unexpected(name) => {
                write!(formatter, "The multipart archive has an unexpected part: {name}")
            }
            Self::Mixed(first, second) => write!(
                formatter,
                "The link holds parts of different archives: {first} and {second}"
            ),
            Self::SpannedZip => formatter.write_str(
                "Split ZIP archives (.z01, .z02 and .zip) are not supported; use a 7z or RAR release",
            ),
            Self::UnsupportedSplit(name) => write!(
                formatter,
                "{name} is a split file of an unsupported format; only .7z.001 and .zip.001 splits are supported"
            ),
            Self::TooMany => write!(
                formatter,
                "The multipart archive has more than {MAX_PARTS} parts"
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Known(PartScheme),
    Spanned,
    UnknownSplit,
}

#[derive(Debug, Clone)]
struct Candidate {
    index: usize,
    kind: Kind,
    /// Lowercase key that every part of one set shares.
    key: String,
    /// The same prefix with its original case, used to name a missing part.
    prefix: String,
    number: u32,
    width: usize,
}

fn digits(value: &str) -> Option<u32> {
    (!value.is_empty() && value.len() <= 4 && value.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| value.parse().ok())
        .flatten()
}

fn classify(index: usize, name: &str) -> Option<Candidate> {
    let lower = name.to_ascii_lowercase();
    let candidate = |kind, cut: usize, number, width| {
        (cut > 0).then(|| Candidate {
            index,
            kind,
            key: lower[..cut].to_owned(),
            prefix: name[..cut].to_owned(),
            number,
            width,
        })
    };
    if let Some(stem) = lower.strip_suffix(".rar")
        && let Some((base, number)) = stem.rsplit_once(".part")
        && let Some(value) = digits(number)
    {
        return candidate(
            Kind::Known(PartScheme::RarVolumes),
            base.len(),
            value,
            number.len(),
        );
    }
    let (base, extension) = lower.rsplit_once('.')?;
    if let Some(number) = extension.strip_prefix('r')
        && matches!(number.len(), 2 | 3)
        && let Some(value) = digits(number)
    {
        return candidate(
            Kind::Known(PartScheme::RarLegacyVolumes),
            base.len(),
            value.checked_add(1)?,
            number.len(),
        );
    }
    if let Some(number) = extension.strip_prefix('z')
        && matches!(number.len(), 2 | 3)
        && let Some(value) = digits(number)
    {
        return candidate(Kind::Spanned, base.len(), value, number.len());
    }
    if extension.len() >= 3
        && let Some(value) = digits(extension)
    {
        let kind = match base.rsplit_once('.').map(|(_, inner)| inner) {
            Some("7z") => Kind::Known(PartScheme::SevenZipSplit),
            Some("zip") => Kind::Known(PartScheme::ZipSplit),
            _ => Kind::UnknownSplit,
        };
        return candidate(kind, base.len(), value, extension.len());
    }
    None
}

/// Whether a single file name belongs to a multipart archive, so it must not be
/// downloaded or extracted on its own.
pub(crate) fn looks_like_part(name: &str) -> bool {
    classify(0, name).is_some()
}

/// The first volume of a legacy RAR set (`game.rar`) and the last one of a spanned
/// ZIP (`game.zip`) only count as parts when a numbered sibling exists.
fn contextual(index: usize, name: &str, numbered: &[Candidate]) -> Option<Candidate> {
    let lower = name.to_ascii_lowercase();
    let (kind, base, number) = match lower.strip_suffix(".rar") {
        Some(base) => (Kind::Known(PartScheme::RarLegacyVolumes), base, 0),
        None => (Kind::Spanned, lower.strip_suffix(".zip")?, u32::MAX),
    };
    let sibling = numbered
        .iter()
        .find(|candidate| candidate.kind == kind && candidate.key == base)?;
    Some(Candidate {
        index,
        kind,
        key: base.to_owned(),
        prefix: name[..base.len()].to_owned(),
        number,
        width: sibling.width,
    })
}

fn part_name(scheme: PartScheme, prefix: &str, number: u32, width: usize) -> String {
    match scheme {
        PartScheme::RarVolumes => format!("{prefix}.part{number:0width$}.rar"),
        PartScheme::RarLegacyVolumes if number == 0 => format!("{prefix}.rar"),
        PartScheme::RarLegacyVolumes => format!("{prefix}.r{:0width$}", number - 1),
        PartScheme::SevenZipSplit | PartScheme::ZipSplit => format!("{prefix}.{number:0width$}"),
    }
}

/// Finds the one complete multipart set among `names`. Names that are not parts are
/// ignored; `Ok(None)` means there is no part at all.
pub(crate) fn find_part_set(names: &[&str]) -> Result<Option<PartSet>, PartSetError> {
    let mut candidates: Vec<Candidate> = names
        .iter()
        .enumerate()
        .filter_map(|(index, name)| classify(index, name))
        .collect();
    let contextual: Vec<Candidate> = names
        .iter()
        .enumerate()
        .filter(|(index, _)| !candidates.iter().any(|found| found.index == *index))
        .filter_map(|(index, name)| contextual(index, name, &candidates))
        .collect();
    candidates.extend(contextual);
    let Some(first) = candidates.first().cloned() else {
        return Ok(None);
    };
    if let Some(other) = candidates
        .iter()
        .find(|candidate| candidate.kind != first.kind || candidate.key != first.key)
    {
        return Err(PartSetError::Mixed(
            names[first.index].to_owned(),
            names[other.index].to_owned(),
        ));
    }
    let scheme = match first.kind {
        Kind::Known(scheme) => scheme,
        Kind::Spanned => return Err(PartSetError::SpannedZip),
        Kind::UnknownSplit => {
            return Err(PartSetError::UnsupportedSplit(
                names[first.index].to_owned(),
            ));
        }
    };
    if candidates.len() > MAX_PARTS {
        return Err(PartSetError::TooMany);
    }
    candidates.sort_by_key(|candidate| (candidate.number, candidate.index));
    let start = u32::from(scheme != PartScheme::RarLegacyVolumes);
    for (expected, (position, candidate)) in (start..).zip(candidates.iter().enumerate()) {
        if position > 0 && candidates[position - 1].number == candidate.number {
            return Err(PartSetError::Duplicate(names[candidate.index].to_owned()));
        }
        if candidate.number < expected {
            return Err(PartSetError::Unexpected(names[candidate.index].to_owned()));
        }
        if candidate.number > expected {
            return Err(PartSetError::Missing(part_name(
                scheme,
                &candidate.prefix,
                expected,
                candidate.width,
            )));
        }
    }
    Ok(Some(PartSet {
        scheme,
        order: candidates.iter().map(|candidate| candidate.index).collect(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(names: &[&str]) -> Result<Option<PartSet>, PartSetError> {
        find_part_set(names)
    }

    fn ordered(names: &[&str]) -> (PartScheme, Vec<String>) {
        let found = set(names).unwrap().unwrap();
        (
            found.scheme,
            found
                .order
                .iter()
                .map(|index| names[*index].to_owned())
                .collect(),
        )
    }

    #[test]
    fn rar_volumes_in_any_order_and_padding() {
        assert_eq!(
            ordered(&["Game.part3.rar", "Game.part1.rar", "Game.part2.rar"]),
            (
                PartScheme::RarVolumes,
                vec![
                    "Game.part1.rar".into(),
                    "Game.part2.rar".into(),
                    "Game.part3.rar".into()
                ]
            )
        );
        let padded: Vec<String> = (1..=12).map(|n| format!("game.part{n:03}.rar")).collect();
        let names: Vec<&str> = padded.iter().map(String::as_str).collect();
        assert_eq!(ordered(&names).1, padded);
    }

    #[test]
    fn mixed_case_names_form_one_set() {
        assert_eq!(
            ordered(&["GAME.PART1.RAR", "game.Part2.rar"]).1,
            vec!["GAME.PART1.RAR", "game.Part2.rar"]
        );
        assert_eq!(
            ordered(&["Game.R00", "Game.RAR"]),
            (
                PartScheme::RarLegacyVolumes,
                vec!["Game.RAR".into(), "Game.R00".into()]
            )
        );
    }

    #[test]
    fn legacy_rar_starts_with_the_rar_volume() {
        assert_eq!(
            ordered(&["game.r01", "game.rar", "game.r00", "readme.txt"]),
            (
                PartScheme::RarLegacyVolumes,
                vec!["game.rar".into(), "game.r00".into(), "game.r01".into()]
            )
        );
        assert_eq!(
            set(&["game.r00", "game.r01"]),
            Err(PartSetError::Missing("game.rar".into()))
        );
    }

    #[test]
    fn byte_splits_of_7z_and_zip() {
        assert_eq!(
            ordered(&["game.7z.002", "game.7z.001"]),
            (
                PartScheme::SevenZipSplit,
                vec!["game.7z.001".into(), "game.7z.002".into()]
            )
        );
        assert_eq!(ordered(&["Game.ZIP.001"]).0, PartScheme::ZipSplit);
    }

    #[test]
    fn gaps_name_the_first_missing_part() {
        assert_eq!(
            set(&["game.part1.rar", "game.part3.rar"]),
            Err(PartSetError::Missing("game.part2.rar".into()))
        );
        assert_eq!(
            set(&["game.part02.rar", "game.part03.rar"]),
            Err(PartSetError::Missing("game.part01.rar".into()))
        );
        assert_eq!(
            set(&["game.rar", "game.r00", "game.r02"]),
            Err(PartSetError::Missing("game.r01".into()))
        );
        assert_eq!(
            set(&["Game.7z.001", "Game.7z.003"]),
            Err(PartSetError::Missing("Game.7z.002".into()))
        );
        assert_eq!(
            set(&["game.zip.002"]),
            Err(PartSetError::Missing("game.zip.001".into()))
        );
    }

    #[test]
    fn duplicates_and_unexpected_numbers_are_rejected() {
        assert_eq!(
            set(&["game.part1.rar", "game.part01.rar"]),
            Err(PartSetError::Duplicate("game.part01.rar".into()))
        );
        assert_eq!(
            set(&["game.part0.rar", "game.part1.rar"]),
            Err(PartSetError::Unexpected("game.part0.rar".into()))
        );
        assert_eq!(
            set(&["game.7z.000", "game.7z.001"]),
            Err(PartSetError::Unexpected("game.7z.000".into()))
        );
    }

    #[test]
    fn parts_of_different_archives_never_mix() {
        assert_eq!(
            set(&["a.part1.rar", "b.part2.rar"]),
            Err(PartSetError::Mixed(
                "a.part1.rar".into(),
                "b.part2.rar".into()
            ))
        );
        assert!(matches!(
            set(&["game.part1.rar", "game.7z.001"]),
            Err(PartSetError::Mixed(..))
        ));
    }

    #[test]
    fn spanned_zip_and_unknown_splits_are_rejected() {
        assert_eq!(
            set(&["game.z01", "game.zip"]),
            Err(PartSetError::SpannedZip)
        );
        assert_eq!(set(&["game.z01"]), Err(PartSetError::SpannedZip));
        assert_eq!(
            set(&["game.rar.001", "game.rar.002"]),
            Err(PartSetError::UnsupportedSplit("game.rar.001".into()))
        );
    }

    #[test]
    fn look_alike_single_files_are_not_parts() {
        for name in [
            "game.rar",
            "game.zip",
            "game.7z",
            "game.part.rar",
            "game.part1a.rar",
            "game.parts1.zip",
            "game.r0",
            "game.r0000",
            "game.7z.01",
            "game.rar5",
            "part1.rar.txt",
            "game_part1.rar",
        ] {
            assert!(!looks_like_part(name), "{name}");
            assert_eq!(set(&[name]), Ok(None), "{name}");
        }
        assert_eq!(set(&[]), Ok(None));
        assert_eq!(set(&["game.zip", "readme.txt"]), Ok(None));
    }

    #[test]
    fn single_parts_look_like_parts() {
        for name in [
            "game.part1.rar",
            "game.part0001.rar",
            "game.r00",
            "game.r123",
            "game.z01",
            "game.7z.001",
            "game.zip.002",
            "game.bin.001",
        ] {
            assert!(looks_like_part(name), "{name}");
        }
    }

    #[test]
    fn a_rar_beside_unrelated_numbered_files_is_not_a_legacy_set() {
        assert_eq!(
            set(&["game.rar", "other.r00"]),
            Err(PartSetError::Missing("other.rar".into()))
        );
    }

    #[test]
    fn too_many_parts_are_refused() {
        let names: Vec<String> = (1..=1000).map(|n| format!("game.7z.{n:04}")).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        assert_eq!(set(&names), Err(PartSetError::TooMany));
    }
}
