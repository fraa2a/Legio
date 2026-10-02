use crate::database::Database;

pub(crate) fn daily_playtime(
    database: &Database,
    day_boundaries: &[i64],
    now: i64,
) -> Result<Vec<i64>, String> {
    if !(2..=32).contains(&day_boundaries.len())
        || day_boundaries
            .iter()
            .any(|boundary| !(0..=253_402_300_799_999).contains(boundary))
        || day_boundaries
            .windows(2)
            .any(|day| !(20 * 3_600_000..=28 * 3_600_000).contains(&(day[1] - day[0])))
    {
        return Err("expected up to 31 ordered local-day intervals".to_owned());
    }

    database.with_connection(|connection| {
        let mut statement = connection
            .prepare(
                "SELECT COALESCE(SUM(MAX(0,
                     MIN(COALESCE(ended_at, ?3), ?2, ?3) - MAX(started_at, ?1))), 0)
                 FROM game_sessions
                 WHERE started_at < ?2 AND COALESCE(ended_at, ?3) > ?1",
            )
            .map_err(|error| format!("could not read playtime activity: {error}"))?;
        day_boundaries
            .windows(2)
            .map(|day| {
                statement
                    .query_row(rusqlite::params![day[0], day[1], now], |row| row.get(0))
                    .map_err(|error| format!("could not read daily playtime: {error}"))
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::CreateGameInput;

    #[test]
    fn activity_splits_sessions_at_local_midnight_and_clips_active_sessions() {
        let directory =
            std::env::temp_dir().join(format!("legio-activity-{}", uuid::Uuid::new_v4()));
        let database = Database::open(&directory).unwrap();
        let game = database
            .create_game(CreateGameInput {
                name: "Game".to_owned(),
                steam_app_id: None,
            })
            .unwrap();
        let hour = 3_600_000;
        database.start_game_session(&game.id, 22 * hour).unwrap();
        database.end_game_session(&game.id, 26 * hour).unwrap();
        database.start_game_session(&game.id, 48 * hour).unwrap();
        assert_eq!(
            daily_playtime(
                &database,
                &[hour, 24 * hour, 49 * hour, 73 * hour],
                50 * hour
            )
            .unwrap(),
            vec![2 * hour, 3 * hour, hour]
        );
        assert_eq!(
            daily_playtime(&database, &[72 * hour, 96 * hour], 50 * hour).unwrap(),
            vec![0]
        );
        drop(database);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn activity_rejects_invalid_day_boundaries() {
        let directory =
            std::env::temp_dir().join(format!("legio-activity-{}", uuid::Uuid::new_v4()));
        let database = Database::open(&directory).unwrap();
        for boundaries in [
            vec![],
            vec![0],
            vec![86_400_000, 0],
            vec![0, 1],
            vec![i64::MIN, i64::MAX],
            vec![0; 33],
        ] {
            assert!(daily_playtime(&database, &boundaries, 0).is_err());
        }
        drop(database);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
