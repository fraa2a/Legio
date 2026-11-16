//! Play statistics for a year or a month, attributed to local days like the activity calendar.

use std::{collections::HashMap, path::Path};

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;

use crate::{
    database::Database,
    playtime_activity::{day_overlaps, validate_day_boundaries},
};

const TOP_GAMES: usize = 5;
const MAX_IMAGE_BYTES: usize = 4 * 1024 * 1024;
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TopGame {
    pub game_id: String,
    pub name: String,
    pub milliseconds: i64,
    pub share: f64,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DayTotal {
    pub date: String,
    pub milliseconds: i64,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LongestSession {
    pub game_id: String,
    pub name: String,
    pub date: String,
    pub milliseconds: i64,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Streak {
    pub start: String,
    pub end: String,
    pub days: u32,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlaytimeRecap {
    pub total_milliseconds: i64,
    pub session_count: u32,
    pub game_count: u32,
    pub top_games: Vec<TopGame>,
    pub longest_day: Option<DayTotal>,
    pub longest_session: Option<LongestSession>,
    pub longest_streak: Option<Streak>,
    /// Monday first.
    pub by_weekday: [i64; 7],
    /// Twelve months for a year, one entry per day for a month.
    pub by_bucket: Vec<i64>,
    pub first_played: Option<String>,
    pub last_played: Option<String>,
}

struct Calendar {
    first_day: i64,
    days: usize,
    monthly: bool,
}

impl Calendar {
    fn new(year: i32, month: Option<u32>) -> Result<Self, String> {
        if !(1970..=9999).contains(&year) {
            return Err("The year is out of range".to_owned());
        }
        let (first_month, days) = match month {
            Some(month @ 1..=12) => (month, days_in_month(year, month)),
            Some(_) => return Err("The month must be between 1 and 12".to_owned()),
            None => (1, if is_leap_year(year) { 366 } else { 365 }),
        };
        Ok(Self {
            first_day: days_from_civil(year, first_month, 1),
            days,
            monthly: month.is_some(),
        })
    }

    fn date(&self, index: usize) -> String {
        let (year, month, day) = civil_from_days(self.first_day + index as i64);
        format!("{year:04}-{month:02}-{day:02}")
    }

    // Monday is 0; 1970-01-01 was a Thursday.
    fn weekday(&self, index: usize) -> usize {
        (self.first_day + index as i64 + 3).rem_euclid(7) as usize
    }

    fn bucket(&self, index: usize) -> usize {
        if self.monthly {
            index
        } else {
            civil_from_days(self.first_day + index as i64).1 as usize - 1
        }
    }

    fn bucket_count(&self) -> usize {
        if self.monthly { self.days } else { 12 }
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i32, month: u32) -> usize {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

// Proleptic Gregorian calendar, days since 1970-01-01.
fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = i64::from(year) - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = i64::from((month + 9) % 12);
    let day_of_year = (153 * month_index + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    } as u32;
    let year = (year_of_era + era * 400) as i32 + i32::from(month <= 2);
    (year, month, day)
}

struct GameTotal {
    name: String,
    milliseconds: i64,
}

pub(crate) fn playtime_recap(
    database: &Database,
    year: i32,
    month: Option<u32>,
    day_boundaries: &[i64],
    now: i64,
) -> Result<PlaytimeRecap, String> {
    let calendar = Calendar::new(year, month)?;
    validate_day_boundaries(day_boundaries, calendar.days)?;
    if day_boundaries.len() != calendar.days + 1 {
        return Err("The day boundaries do not match the period".to_owned());
    }
    let (period_start, period_end) = (day_boundaries[0], day_boundaries[calendar.days]);

    let mut daily = vec![0i64; calendar.days];
    let mut games: HashMap<String, GameTotal> = HashMap::new();
    let mut session_count = 0u32;
    let mut longest_session: Option<LongestSession> = None;
    database.with_connection(|connection| {
        let mut statement = connection
            .prepare(
                "SELECT s.game_id, COALESCE(g.name_override, g.automatic_name), s.started_at,
                     MIN(COALESCE(s.ended_at, ?3), ?3)
                 FROM game_sessions s JOIN games g ON g.id = s.game_id
                 WHERE s.started_at < ?2 AND COALESCE(s.ended_at, ?3) > ?1
                 ORDER BY s.started_at, s.id",
            )
            .map_err(read_error)?;
        let rows = statement
            .query_map(rusqlite::params![period_start, period_end, now], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .map_err(read_error)?;
        for row in rows {
            let (game_id, name, start, end) = row.map_err(read_error)?;
            let (start, end) = (start.max(period_start), end.min(period_end));
            if end <= start {
                continue;
            }
            session_count += 1;
            let duration = end - start;
            if longest_session
                .as_ref()
                .is_none_or(|best| duration > best.milliseconds)
            {
                let day = day_overlaps(day_boundaries, start, end)
                    .next()
                    .map_or(0, |(index, _)| index);
                longest_session = Some(LongestSession {
                    game_id: game_id.clone(),
                    name: name.clone(),
                    date: calendar.date(day),
                    milliseconds: duration,
                });
            }
            let entry = games.entry(game_id).or_insert(GameTotal {
                name,
                milliseconds: 0,
            });
            entry.milliseconds = entry
                .milliseconds
                .checked_add(duration)
                .ok_or("Playtime exceeded the supported range")?;
            for (index, overlap) in day_overlaps(day_boundaries, start, end) {
                daily[index] += overlap;
            }
        }
        Ok(())
    })?;

    let total_milliseconds: i64 = daily.iter().sum();
    let mut ranked: Vec<(String, GameTotal)> = games.into_iter().collect();
    ranked.sort_by(|left, right| {
        right
            .1
            .milliseconds
            .cmp(&left.1.milliseconds)
            .then_with(|| left.1.name.cmp(&right.1.name))
            .then_with(|| left.0.cmp(&right.0))
    });
    let game_count = ranked.len() as u32;
    let top_games = ranked
        .into_iter()
        .take(TOP_GAMES)
        .map(|(game_id, game)| TopGame {
            game_id,
            name: game.name,
            milliseconds: game.milliseconds,
            share: game.milliseconds as f64 / total_milliseconds as f64,
        })
        .collect();

    let mut by_weekday = [0i64; 7];
    let mut by_bucket = vec![0i64; calendar.bucket_count()];
    for (index, milliseconds) in daily.iter().enumerate() {
        by_weekday[calendar.weekday(index)] += milliseconds;
        by_bucket[calendar.bucket(index)] += milliseconds;
    }

    // The earliest day wins a tie.
    let longest_day = daily
        .iter()
        .enumerate()
        .filter(|(_, milliseconds)| **milliseconds > 0)
        .reduce(|best, day| if day.1 > best.1 { day } else { best })
        .map(|(index, milliseconds)| DayTotal {
            date: calendar.date(index),
            milliseconds: *milliseconds,
        });
    let played = |milliseconds: &i64| *milliseconds > 0;
    Ok(PlaytimeRecap {
        total_milliseconds,
        session_count,
        game_count,
        top_games,
        longest_day,
        longest_session,
        longest_streak: longest_streak(&daily).map(|(start, days)| Streak {
            start: calendar.date(start),
            end: calendar.date(start + days - 1),
            days: days as u32,
        }),
        by_weekday,
        by_bucket,
        first_played: daily
            .iter()
            .position(played)
            .map(|index| calendar.date(index)),
        last_played: daily
            .iter()
            .rposition(played)
            .map(|index| calendar.date(index)),
    })
}

/// Start index and length of the earliest longest run of played days.
fn longest_streak(daily: &[i64]) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    let mut run_start = 0;
    for index in 0..=daily.len() {
        if index < daily.len() && daily[index] > 0 {
            continue;
        }
        let length = index - run_start;
        if length > best.map_or(0, |(_, best_length)| best_length) {
            best = Some((run_start, length));
        }
        run_start = index + 1;
    }
    best
}

/// Calendar years that have recorded sessions, newest first.
pub(crate) fn playtime_years(database: &Database, now: i64) -> Result<Vec<i32>, String> {
    database.with_connection(|connection| {
        let mut statement = connection
            .prepare(
                "SELECT DISTINCT year FROM (
                     SELECT CAST(strftime('%Y', started_at / 1000, 'unixepoch', 'localtime') AS INTEGER) AS year
                         FROM game_sessions WHERE COALESCE(ended_at, ?1) > started_at
                     UNION
                     SELECT CAST(strftime('%Y', (COALESCE(ended_at, ?1) - 1) / 1000, 'unixepoch', 'localtime') AS INTEGER)
                         FROM game_sessions WHERE COALESCE(ended_at, ?1) > started_at
                 ) WHERE year IS NOT NULL ORDER BY year DESC",
            )
            .map_err(read_error)?;
        let rows = statement
            .query_map([now], |row| row.get::<_, i32>(0))
            .map_err(read_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(read_error)
    })
}

/// Writes a PNG sent by the frontend as base64 to a path the user chose.
pub(crate) fn write_recap_image(path: &Path, encoded: &str) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("Image paths must be absolute".to_owned());
    }
    if encoded.len() > MAX_IMAGE_BYTES / 3 * 4 + 4 {
        return Err("The image is too large".to_owned());
    }
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| "The image data is not valid".to_owned())?;
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err("The image is too large".to_owned());
    }
    if !bytes.starts_with(&PNG_SIGNATURE) {
        return Err("The image is not a PNG".to_owned());
    }
    crate::atomic_file::write_atomic(path, &bytes)
        .map_err(|error| format!("Could not write the image: {error}"))
}

fn read_error(error: rusqlite::Error) -> String {
    format!("could not read the playtime recap: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::CreateGameInput;

    const HOUR: i64 = 3_600_000;
    const DAY: i64 = 24 * HOUR;

    struct Fixture {
        directory: std::path::PathBuf,
        database: Database,
    }

    impl Fixture {
        fn new() -> Self {
            let directory =
                std::env::temp_dir().join(format!("legio-recap-{}", uuid::Uuid::new_v4()));
            let database = Database::open(&directory).unwrap();
            Self {
                directory,
                database,
            }
        }

        fn game(&self, name: &str) -> String {
            self.database
                .create_game(CreateGameInput {
                    name: name.to_owned(),
                    steam_app_id: None,
                })
                .unwrap()
                .id
        }

        fn session(&self, game: &str, start: i64, end: i64) {
            self.database.start_game_session(game, start).unwrap();
            self.database.end_game_session(game, end).unwrap();
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.directory).ok();
        }
    }

    // UTC midnights for every day of the period.
    fn boundaries(year: i32, month: Option<u32>) -> Vec<i64> {
        let calendar = Calendar::new(year, month).unwrap();
        (0..=calendar.days as i64)
            .map(|day| (calendar.first_day + day) * DAY)
            .collect()
    }

    fn at(year: i32, month: u32, day: u32) -> i64 {
        days_from_civil(year, month, day) * DAY
    }

    #[test]
    fn calendar_dates_and_weekdays_are_correct() {
        let calendar = Calendar::new(2024, None).unwrap();
        assert_eq!(calendar.days, 366);
        assert_eq!(calendar.date(0), "2024-01-01");
        assert_eq!(calendar.date(59), "2024-02-29");
        assert_eq!(calendar.date(365), "2024-12-31");
        // 2024-01-01 was a Monday and 2024-02-29 a Thursday.
        assert_eq!(calendar.weekday(0), 0);
        assert_eq!(calendar.weekday(59), 3);
        assert_eq!(Calendar::new(2023, Some(2)).unwrap().days, 28);
        assert!(Calendar::new(2024, Some(13)).is_err());
        assert!(Calendar::new(1969, None).is_err());
    }

    #[test]
    fn empty_period_has_no_records() {
        let fixture = Fixture::new();
        let recap = playtime_recap(
            &fixture.database,
            2024,
            None,
            &boundaries(2024, None),
            at(2025, 1, 1),
        )
        .unwrap();
        assert_eq!(recap.total_milliseconds, 0);
        assert_eq!(recap.session_count, 0);
        assert_eq!(recap.game_count, 0);
        assert!(recap.top_games.is_empty());
        assert!(recap.longest_day.is_none());
        assert!(recap.longest_session.is_none());
        assert!(recap.longest_streak.is_none());
        assert!(recap.first_played.is_none() && recap.last_played.is_none());
        assert_eq!(recap.by_bucket, vec![0; 12]);
        assert_eq!(recap.by_weekday, [0; 7]);
    }

    #[test]
    fn month_recap_aggregates_days_weekdays_and_records() {
        let fixture = Fixture::new();
        let (alpha, beta) = (fixture.game("Alpha"), fixture.game("Beta"));
        // Monday 2024-01-01 and Tuesday 2024-01-02, then Friday 2024-01-05.
        fixture.session(
            &alpha,
            at(2024, 1, 1) + 20 * HOUR,
            at(2024, 1, 1) + 23 * HOUR,
        );
        fixture.session(&beta, at(2024, 1, 2) + HOUR, at(2024, 1, 2) + 2 * HOUR);
        fixture.session(
            &alpha,
            at(2024, 1, 5) + 10 * HOUR,
            at(2024, 1, 5) + 12 * HOUR,
        );
        let recap = playtime_recap(
            &fixture.database,
            2024,
            Some(1),
            &boundaries(2024, Some(1)),
            at(2024, 2, 1),
        )
        .unwrap();
        assert_eq!(recap.total_milliseconds, 6 * HOUR);
        assert_eq!(recap.session_count, 3);
        assert_eq!(recap.game_count, 2);
        assert_eq!(recap.top_games[0].name, "Alpha");
        assert_eq!(recap.top_games[0].milliseconds, 5 * HOUR);
        assert!((recap.top_games[0].share - 5.0 / 6.0).abs() < 1e-9);
        assert_eq!(
            recap.longest_day,
            Some(DayTotal {
                date: "2024-01-01".to_owned(),
                milliseconds: 3 * HOUR
            })
        );
        let session = recap.longest_session.unwrap();
        assert_eq!(
            (session.name.as_str(), session.milliseconds),
            ("Alpha", 3 * HOUR)
        );
        assert_eq!(session.date, "2024-01-01");
        assert_eq!(
            recap.longest_streak,
            Some(Streak {
                start: "2024-01-01".to_owned(),
                end: "2024-01-02".to_owned(),
                days: 2
            })
        );
        assert_eq!(recap.by_weekday, [3 * HOUR, HOUR, 0, 0, 2 * HOUR, 0, 0]);
        assert_eq!(recap.by_bucket.len(), 31);
        assert_eq!(recap.by_bucket[4], 2 * HOUR);
        assert_eq!(recap.first_played.as_deref(), Some("2024-01-01"));
        assert_eq!(recap.last_played.as_deref(), Some("2024-01-05"));
    }

    #[test]
    fn midnight_session_splits_across_days_and_months() {
        let fixture = Fixture::new();
        let game = fixture.game("Night");
        // 2024-01-31 22:00 to 2024-02-01 02:00.
        fixture.session(
            &game,
            at(2024, 1, 31) + 22 * HOUR,
            at(2024, 2, 1) + 2 * HOUR,
        );
        let january = playtime_recap(
            &fixture.database,
            2024,
            Some(1),
            &boundaries(2024, Some(1)),
            at(2024, 3, 1),
        )
        .unwrap();
        assert_eq!(january.total_milliseconds, 2 * HOUR);
        assert_eq!(january.session_count, 1);
        assert_eq!(january.longest_session.unwrap().milliseconds, 2 * HOUR);
        let year = playtime_recap(
            &fixture.database,
            2024,
            None,
            &boundaries(2024, None),
            at(2024, 3, 1),
        )
        .unwrap();
        assert_eq!(year.total_milliseconds, 4 * HOUR);
        assert_eq!(year.by_bucket[..2], [2 * HOUR, 2 * HOUR]);
        let streak = year.longest_streak.unwrap();
        assert_eq!(
            (streak.start.as_str(), streak.end.as_str()),
            ("2024-01-31", "2024-02-01")
        );
    }

    #[test]
    fn streak_crosses_months_but_stops_at_the_year_edge() {
        let fixture = Fixture::new();
        let game = fixture.game("Daily");
        for (month, day) in [(12, 30), (12, 31), (1, 1), (1, 2)] {
            let year = if month == 12 { 2023 } else { 2024 };
            fixture.session(
                &game,
                at(year, month, day) + HOUR,
                at(year, month, day) + 2 * HOUR,
            );
        }
        let now = at(2024, 6, 1);
        let december =
            playtime_recap(&fixture.database, 2023, None, &boundaries(2023, None), now).unwrap();
        assert_eq!(december.longest_streak.unwrap().days, 2);
        let next =
            playtime_recap(&fixture.database, 2024, None, &boundaries(2024, None), now).unwrap();
        let streak = next.longest_streak.unwrap();
        assert_eq!((streak.start.as_str(), streak.days), ("2024-01-01", 2));
        // Across Jan 31 and Feb 1 inside one year.
        let february_fixture = Fixture::new();
        let february = february_fixture.game("Feb");
        for (month, day) in [(1, 30), (1, 31), (2, 1), (2, 2), (2, 3)] {
            february_fixture.session(
                &february,
                at(2024, month, day) + HOUR,
                at(2024, month, day) + 2 * HOUR,
            );
        }
        let recap = playtime_recap(
            &february_fixture.database,
            2024,
            None,
            &boundaries(2024, None),
            now,
        )
        .unwrap();
        let streak = recap.longest_streak.unwrap();
        assert_eq!(
            (streak.start.as_str(), streak.end.as_str(), streak.days),
            ("2024-01-30", "2024-02-03", 5)
        );
    }

    #[test]
    fn equal_streaks_and_days_prefer_the_earliest() {
        assert_eq!(longest_streak(&[0, 1, 1, 0, 1, 1, 0]), Some((1, 2)));
        assert_eq!(longest_streak(&[1, 1, 1]), Some((0, 3)));
        assert_eq!(longest_streak(&[0, 0]), None);
        assert_eq!(longest_streak(&[]), None);
    }

    #[test]
    fn top_games_are_limited_and_ties_sort_by_name() {
        let fixture = Fixture::new();
        for (index, name) in ["Zeta", "Alpha", "Mid", "Bravo", "Echo", "Delta"]
            .iter()
            .enumerate()
        {
            let game = fixture.game(name);
            let hours = if index < 2 { 2 } else { 1 };
            fixture.session(
                &game,
                at(2024, 3, 1 + index as u32) + HOUR,
                at(2024, 3, 1 + index as u32) + (1 + hours) * HOUR,
            );
        }
        let recap = playtime_recap(
            &fixture.database,
            2024,
            Some(3),
            &boundaries(2024, Some(3)),
            at(2024, 4, 1),
        )
        .unwrap();
        assert_eq!(recap.game_count, 6);
        let names: Vec<_> = recap
            .top_games
            .iter()
            .map(|game| game.name.as_str())
            .collect();
        assert_eq!(names, ["Alpha", "Zeta", "Bravo", "Delta", "Echo"]);
        assert_eq!(recap.longest_day.unwrap().date, "2024-03-01");
    }

    #[test]
    fn dst_day_keeps_its_own_length() {
        let fixture = Fixture::new();
        let game = fixture.game("Spring");
        // Rome midnights in March 2024: UTC+1 until the switch on the 31st, then UTC+2.
        let mut march = boundaries(2024, Some(3));
        for (index, boundary) in march.iter_mut().enumerate() {
            *boundary -= if index <= 30 { HOUR } else { 2 * HOUR };
        }
        assert_eq!(march[31] - march[30], 23 * HOUR);
        fixture.session(&game, march[30] - HOUR, march[30] + 2 * HOUR);
        fixture.session(&game, march[30] + 3 * HOUR, march[31] - HOUR);
        let recap =
            playtime_recap(&fixture.database, 2024, Some(3), &march, at(2024, 4, 1)).unwrap();
        assert_eq!(recap.by_bucket[29], HOUR);
        assert_eq!(recap.by_bucket[30], 2 * HOUR + 19 * HOUR);
        assert_eq!(recap.longest_day.unwrap().date, "2024-03-31");
    }

    #[test]
    fn open_sessions_run_until_now_and_mismatched_boundaries_fail() {
        let fixture = Fixture::new();
        let game = fixture.game("Open");
        fixture
            .database
            .start_game_session(&game, at(2024, 5, 1) + HOUR)
            .unwrap();
        let recap = playtime_recap(
            &fixture.database,
            2024,
            Some(5),
            &boundaries(2024, Some(5)),
            at(2024, 5, 1) + 3 * HOUR,
        )
        .unwrap();
        assert_eq!(recap.total_milliseconds, 2 * HOUR);
        assert!(
            playtime_recap(
                &fixture.database,
                2024,
                Some(5),
                &boundaries(2024, Some(4)),
                0
            )
            .is_err()
        );
        assert!(playtime_recap(&fixture.database, 2024, Some(5), &[], 0).is_err());
    }

    #[test]
    fn years_list_only_years_with_sessions() {
        let fixture = Fixture::new();
        assert!(
            playtime_years(&fixture.database, at(2024, 1, 1))
                .unwrap()
                .is_empty()
        );
        let game = fixture.game("Years");
        // Mid-June so the local time zone cannot move the year.
        fixture.session(&game, at(2021, 6, 15), at(2021, 6, 15) + HOUR);
        fixture.session(&game, at(2023, 6, 15), at(2023, 6, 15) + HOUR);
        assert_eq!(
            playtime_years(&fixture.database, at(2024, 1, 1)).unwrap(),
            [2023, 2021]
        );
    }

    #[test]
    fn image_writer_accepts_only_bounded_pngs_at_absolute_paths() {
        let directory =
            std::env::temp_dir().join(format!("legio-recap-image-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("recap.png");
        let mut png = PNG_SIGNATURE.to_vec();
        png.extend_from_slice(b"data");
        write_recap_image(&path, &STANDARD.encode(&png)).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), png);
        assert!(write_recap_image(&path, &STANDARD.encode(b"GIF89a")).is_err());
        assert!(write_recap_image(&path, "%%%").is_err());
        assert!(write_recap_image(Path::new("recap.png"), &STANDARD.encode(&png)).is_err());
        let mut large = PNG_SIGNATURE.to_vec();
        large.resize(MAX_IMAGE_BYTES + 1, 0);
        assert!(write_recap_image(&path, &STANDARD.encode(&large)).is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
