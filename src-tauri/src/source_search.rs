use crate::source_matching::{MatchResult, normalized_title, text_candidate_matches};
use std::time::{Duration, Instant};

pub(crate) fn find_matches(
    releases: &[String],
    games: &[(u32, String)],
) -> Result<Vec<MatchResult>, String> {
    let mut matches = vec![MatchResult::Unresolved; releases.len()];
    if releases.is_empty() || games.is_empty() {
        return Ok(matches);
    }
    if releases.len() > 100_000 || games.len() > 100_000 {
        return Err("Source text index exceeds the game limit".into());
    }
    let titles: Vec<_> = releases
        .iter()
        .map(|title| normalized_title(title))
        .collect();
    let runtime = rquickjs::Runtime::new().map_err(|_| "Cannot create source search runtime")?;
    runtime.set_memory_limit(128 * 1024 * 1024);
    runtime.set_max_stack_size(512 * 1024);
    let deadline = Instant::now() + Duration::from_secs(10);
    runtime.set_interrupt_handler(Some(Box::new(move || Instant::now() >= deadline)));
    let context =
        rquickjs::Context::full(&runtime).map_err(|_| "Cannot create source search context")?;
    context.with(|ctx| {
        ctx.eval::<(), _>(include_str!("../vendor/flexsearch.bundle.min.js"))
            .map_err(|_| "Cannot load FlexSearch")?;
        ctx.eval::<(), _>(include_str!("source_search.js"))
            .map_err(|_| "Cannot load source search adapter")?;
        let input = serde_json::to_string(&titles).map_err(|error| error.to_string())?;
        ctx.eval::<(), _>(format!("legioBuildReleaseIndex({input});"))
            .map_err(|_| "Source text indexing exceeded its resource limit")?;
        for (app_id, name) in games {
            if Instant::now() >= deadline {
                return Err("Source text search exceeded its time limit".into());
            }
            let query = normalized_title(name);
            if *app_id == 0 || query.is_empty() || query.len() > 512 {
                continue;
            }
            let input = serde_json::to_string(&query).map_err(|error| error.to_string())?;
            let result: String = ctx
                .eval(format!("legioSearchReleases({input})"))
                .map_err(|_| "Source text search exceeded its resource limit")?;
            let positions: Vec<usize> = serde_json::from_str(&result)
                .map_err(|_| "Source text search returned invalid results")?;
            for position in positions {
                let title = titles
                    .get(position)
                    .ok_or("Invalid source search position")?;
                if !text_candidate_matches(title, &query) {
                    continue;
                }
                let candidate = &mut matches[position];
                *candidate = match *candidate {
                    MatchResult::Unresolved => MatchResult::Matched(*app_id),
                    MatchResult::Matched(id) if id == *app_id => MatchResult::Matched(id),
                    _ => MatchResult::Ambiguous,
                };
            }
        }
        Ok(matches)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn searches_all_releases_without_assigning_generic_title_subsets() {
        let releases = vec![
            "Machine Human Resource SteamGG Repack".into(),
            "Free Download SIMULACRA".into(),
            "Portal Stories Mel".into(),
            "Portal 2 SteamGG Repack".into(),
        ];
        let games = vec![
            (1, "Human Resource Machine".into()),
            (2, "SIMULACRA".into()),
            (3, "Portal".into()),
        ];
        assert_eq!(
            find_matches(&releases, &games).unwrap(),
            vec![
                MatchResult::Matched(1),
                MatchResult::Matched(2),
                MatchResult::Unresolved,
                MatchResult::Unresolved,
            ]
        );
        let repeated = vec!["Same Game Release".into(); 160];
        assert_eq!(
            find_matches(&repeated, &[(5, "Same Game Release".into())]).unwrap(),
            vec![MatchResult::Matched(5); 160]
        );
    }

    #[test]
    fn retains_ambiguity_and_deduplicates_catalogue_ids() {
        let releases = vec!["Machine Human Resource".into()];
        assert_eq!(
            find_matches(
                &releases,
                &[
                    (1, "Human Resource Machine".into()),
                    (1, "Resource Machine Human".into())
                ]
            )
            .unwrap(),
            vec![MatchResult::Matched(1)]
        );
        assert_eq!(
            find_matches(
                &releases,
                &[
                    (1, "Human Resource Machine".into()),
                    (2, "Resource Machine Human".into())
                ]
            )
            .unwrap(),
            vec![MatchResult::Ambiguous]
        );
    }
}
