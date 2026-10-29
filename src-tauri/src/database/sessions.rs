use super::{Database, PlaytimeSummary, database_error, parse_game_id};
use rusqlite::{OptionalExtension, params};

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RunningGamePresence {
    pub(crate) game_id: String,
    pub(crate) name: String,
    pub(crate) started_at: Option<i64>,
    pub(crate) steam_app_id: Option<u32>,
}

impl Database {
    pub(crate) fn start_game_session(&self, game_id: &str, now: i64) -> Result<(), String> {
        let game_id = parse_game_id(game_id)?;
        self.with_connection(|connection| {
            connection
                .execute(
                    "INSERT INTO game_sessions (game_id, started_at, last_seen_at)
                     VALUES (?1, ?2, ?2)",
                    params![game_id, now],
                )
                .map_err(database_error)?;
            Ok(())
        })
    }

    pub(crate) fn heartbeat_game_session(&self, game_id: &str, now: i64) -> Result<(), String> {
        let game_id = parse_game_id(game_id)?;
        self.with_connection(|connection| {
            connection
                .execute(
                    "UPDATE game_sessions SET last_seen_at = ?2
                     WHERE game_id = ?1 AND ended_at IS NULL",
                    params![game_id, now],
                )
                .map_err(database_error)?;
            Ok(())
        })
    }

    pub(crate) fn end_game_session(&self, game_id: &str, now: i64) -> Result<(), String> {
        let game_id = parse_game_id(game_id)?;
        self.with_connection(|connection| {
            connection
                .execute(
                    "UPDATE game_sessions SET ended_at = ?2, last_seen_at = ?2,
                         end_reason = 'finished'
                     WHERE game_id = ?1 AND ended_at IS NULL",
                    params![game_id, now],
                )
                .map_err(database_error)?;
            Ok(())
        })
    }

    pub(crate) fn set_playtime(&self, game_id: &str, total: i64, now: i64) -> Result<(), String> {
        let id = parse_game_id(game_id)?;
        if !(0..=9_007_199_254_740_991).contains(&total) {
            return Err("Playtime must be a non-negative safe integer in milliseconds".into());
        }
        self.with_connection(|connection| {
            let tx = connection.unchecked_transaction().map_err(database_error)?;
            let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM games WHERE id=?1)", [&id], |r|r.get(0)).map_err(database_error)?;
            if !exists { return Err("game was not found".into()); }
            let (recorded,active): (i64,bool) = tx.query_row("SELECT COALESCE(SUM(MAX(0,COALESCE(ended_at,?2)-started_at)),0), COALESCE(MAX(ended_at IS NULL),0) FROM game_sessions WHERE game_id=?1",params![id,now],|r|Ok((r.get(0)?,r.get(1)?))).map_err(database_error)?;
            if active { return Err("Close the game before changing its playtime".into()); }
            let adjustment = total.checked_sub(recorded).ok_or("Playtime exceeds the supported range")?;
            tx.execute("UPDATE games SET playtime_adjustment=?2 WHERE id=?1",params![id,adjustment]).map_err(database_error)?;
            tx.commit().map_err(database_error)
        })
    }

    pub(crate) fn playtime_summaries(&self, now: i64) -> Result<Vec<PlaytimeSummary>, String> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT g.id,
                         MAX(0, COALESCE(SUM(CASE WHEN s.ended_at IS NULL
                             THEN MAX(0, ?1 - s.started_at)
                             ELSE MAX(0, s.ended_at - s.started_at) END), 0) + g.playtime_adjustment),
                         SUM(CASE WHEN s.id IS NOT NULL AND s.ended_at IS NULL THEN 1 ELSE 0 END),
                         MAX(s.started_at)
                     FROM games g LEFT JOIN game_sessions s ON s.game_id = g.id
                     GROUP BY g.id ORDER BY g.id",
                )
                .map_err(database_error)?;
            let rows = statement
                .query_map([now], |row| {
                    Ok(PlaytimeSummary {
                        game_id: row.get(0)?,
                        total_milliseconds: row.get(1)?,
                        active_sessions: row.get::<_, Option<u32>>(2)?.unwrap_or(0),
                        last_played_at: row.get(3)?,
                    })
                })
                .map_err(database_error)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(database_error)
        })
    }

    pub(crate) fn running_game_presence(
        &self,
        game_ids: &[&str],
    ) -> Result<Option<RunningGamePresence>, String> {
        if game_ids.is_empty() {
            return Ok(None);
        }
        let placeholders = (1..=game_ids.len())
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(",");
        self.with_connection(|connection| {
            connection.query_row(&format!(
                "SELECT g.id, COALESCE(g.name_override, g.automatic_name), g.steam_app_id,
                    CASE WHEN SUM(CASE WHEN s.id IS NOT NULL AND s.ended_at IS NULL THEN 1 ELSE 0 END) > 0 THEN MAX(s.started_at) END
                 FROM games g LEFT JOIN game_sessions s ON s.game_id = g.id
                 WHERE g.id IN ({placeholders}) GROUP BY g.id
                 ORDER BY MAX(s.started_at) DESC, g.id DESC LIMIT 1"),
                rusqlite::params_from_iter(game_ids.iter()), |row| Ok(RunningGamePresence {
                    game_id: row.get(0)?,
                    name: row.get(1)?,
                    steam_app_id: row.get(2)?,
                    started_at: row.get(3)?,
                }))
                .optional().map_err(database_error)
        })
    }

    pub(super) fn recover_open_game_sessions(&self, now: i64) -> Result<(), String> {
        self.with_connection(|connection| {
            connection
                .execute(
                    "UPDATE game_sessions SET ended_at = last_seen_at,
                         end_reason = 'interrupted'
                     WHERE ended_at IS NULL AND last_seen_at <= ?1",
                    [now],
                )
                .map_err(database_error)?;
            Ok(())
        })
    }
}
