use super::{SCHEMA_VERSION, database_error};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(database_error)?;
    if version > SCHEMA_VERSION {
        return Err(format!(
            "local database schema version {version} is newer than this Legio build supports"
        ));
    }
    if version == SCHEMA_VERSION {
        return Ok(());
    }

    let transaction = connection.unchecked_transaction().map_err(database_error)?;
    if version == 0 {
        transaction
            .execute_batch(
                "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE games (
                   id TEXT PRIMARY KEY NOT NULL,
                   steam_app_id INTEGER,
                   automatic_name TEXT,
                   name_override TEXT,
                   CHECK (steam_app_id IS NULL OR steam_app_id > 0)
                 );
                 CREATE INDEX games_steam_app_id_idx ON games (steam_app_id);
                 PRAGMA user_version = 1;",
            )
            .map_err(database_error)?;
    }
    if version < 2 {
        transaction
            .execute_batch(
                "CREATE TABLE catalog_games (
                steam_app_id INTEGER PRIMARY KEY CHECK (steam_app_id BETWEEN 1 AND 4294967295),
                name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 512),
                search_name TEXT NOT NULL,
                fetched_at INTEGER NOT NULL
             );
             CREATE TABLE catalog_cache (
                provider TEXT PRIMARY KEY CHECK (provider = 'hydra'),
                query TEXT NOT NULL,
                fetched_at INTEGER NOT NULL,
                remote_count INTEGER NOT NULL CHECK (remote_count >= 0)
             );
             PRAGMA user_version = 2;",
            )
            .map_err(database_error)?;
    }
    if version < 3 {
        transaction
            .execute_batch(
                "ALTER TABLE games ADD COLUMN steam_install_path TEXT;
                 PRAGMA user_version = 3;",
            )
            .map_err(database_error)?;
    }
    if version < 4 {
        transaction
            .execute_batch(
                "CREATE TABLE steam_details_cache (
                steam_app_id INTEGER PRIMARY KEY CHECK (steam_app_id BETWEEN 1 AND 4294967295),
                details TEXT NOT NULL CHECK (length(CAST(details AS BLOB)) BETWEEN 1 AND 65536),
                fetched_at INTEGER NOT NULL CHECK (fetched_at >= 0)
            );
            PRAGMA user_version = 4;",
            )
            .map_err(database_error)?;
    }
    if version < 5 {
        transaction
            .execute_batch(
                "ALTER TABLE games ADD COLUMN steam_account_id TEXT;
             PRAGMA user_version = 5;",
            )
            .map_err(database_error)?;
    }
    if version < 6 {
        transaction
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS legio_source_cache (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    manifest BLOB NOT NULL CHECK (length(manifest) BETWEEN 1 AND 2097152),
                    fetched_at INTEGER NOT NULL CHECK (fetched_at >= 0)
                );
                PRAGMA user_version = 6;",
            )
            .map_err(database_error)?;
    }
    if version < 7 {
        transaction.execute_batch(
            "CREATE TABLE downloads (
                id TEXT PRIMARY KEY NOT NULL,
                steam_app_id INTEGER NOT NULL CHECK (steam_app_id BETWEEN 1 AND 4294967295),
                name TEXT NOT NULL,
                release_version TEXT NOT NULL,
                url TEXT NOT NULL,
                sha256 TEXT NOT NULL,
                etag TEXT,
                size_bytes INTEGER NOT NULL CHECK (size_bytes > 0),
                downloaded_bytes INTEGER NOT NULL DEFAULT 0 CHECK (downloaded_bytes >= 0 AND downloaded_bytes <= size_bytes),
                speed_bps INTEGER NOT NULL DEFAULT 0,
                eta_seconds INTEGER,
                status TEXT NOT NULL CHECK (status IN ('queued', 'downloading', 'paused', 'waiting', 'failed', 'downloaded', 'cancelled')),
                error TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
             );
             CREATE INDEX downloads_status_idx ON downloads (status, created_at);
             PRAGMA user_version = 7;"
        ).map_err(database_error)?;
    }
    if version < 8 {
        transaction.execute_batch(
            "ALTER TABLE downloads ADD COLUMN staged_path TEXT;
             CREATE TABLE downloads_v8 (
                id TEXT PRIMARY KEY NOT NULL,
                steam_app_id INTEGER NOT NULL CHECK (steam_app_id BETWEEN 1 AND 4294967295),
                name TEXT NOT NULL,
                release_version TEXT NOT NULL,
                url TEXT NOT NULL,
                sha256 TEXT NOT NULL,
                etag TEXT,
                size_bytes INTEGER NOT NULL CHECK (size_bytes > 0),
                downloaded_bytes INTEGER NOT NULL DEFAULT 0 CHECK (downloaded_bytes >= 0 AND downloaded_bytes <= size_bytes),
                speed_bps INTEGER NOT NULL DEFAULT 0,
                eta_seconds INTEGER,
                status TEXT NOT NULL CHECK (status IN ('queued', 'downloading', 'paused', 'waiting', 'failed', 'downloaded', 'staging', 'staged', 'cancelled')),
                error TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                staged_path TEXT
             );
             INSERT INTO downloads_v8 SELECT id, steam_app_id, name, release_version, url, sha256, etag, size_bytes, downloaded_bytes, speed_bps, eta_seconds, status, error, created_at, updated_at, staged_path FROM downloads;
             DROP TABLE downloads;
             ALTER TABLE downloads_v8 RENAME TO downloads;
             CREATE INDEX downloads_status_idx ON downloads (status, created_at);
             PRAGMA user_version = 8;"
        ).map_err(database_error)?;
    }
    if version < 9 {
        transaction
            .execute_batch(
                "ALTER TABLE games ADD COLUMN executable_path TEXT;
                 CREATE UNIQUE INDEX games_executable_path_idx ON games (executable_path) WHERE executable_path IS NOT NULL;
                 PRAGMA user_version = 9;",
            )
            .map_err(database_error)?;
    }
    if version < 10 {
        transaction.execute_batch(
            "ALTER TABLE downloads ADD COLUMN final_path TEXT;
             ALTER TABLE downloads ADD COLUMN executable_relative TEXT;
             ALTER TABLE downloads ADD COLUMN install_token TEXT;
             CREATE TABLE downloads_v10 (
                id TEXT PRIMARY KEY NOT NULL,
                steam_app_id INTEGER NOT NULL CHECK (steam_app_id BETWEEN 1 AND 4294967295),
                name TEXT NOT NULL,
                release_version TEXT NOT NULL,
                url TEXT NOT NULL,
                sha256 TEXT NOT NULL,
                etag TEXT,
                size_bytes INTEGER NOT NULL CHECK (size_bytes > 0),
                downloaded_bytes INTEGER NOT NULL DEFAULT 0 CHECK (downloaded_bytes >= 0 AND downloaded_bytes <= size_bytes),
                speed_bps INTEGER NOT NULL DEFAULT 0,
                eta_seconds INTEGER,
                status TEXT NOT NULL CHECK (status IN ('queued', 'downloading', 'paused', 'waiting', 'failed', 'downloaded', 'staging', 'staged', 'finalizing', 'installed', 'cancelled')),
                error TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                staged_path TEXT,
                final_path TEXT,
                executable_relative TEXT,
                install_token TEXT
             );
             INSERT INTO downloads_v10 SELECT id, steam_app_id, name, release_version, url, sha256, etag, size_bytes, downloaded_bytes, speed_bps, eta_seconds, status, error, created_at, updated_at, staged_path, final_path, executable_relative, install_token FROM downloads;
             DROP TABLE downloads;
             ALTER TABLE downloads_v10 RENAME TO downloads;
             CREATE INDEX downloads_status_idx ON downloads (status, created_at);
             PRAGMA user_version = 10;"
        ).map_err(database_error)?;
    }
    if version < 11 {
        transaction
            .execute_batch(
                "CREATE TABLE compatibility_defaults (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    runner_path TEXT,
                    prefix_root TEXT,
                    arguments_before TEXT NOT NULL,
                    arguments_after TEXT NOT NULL,
                    working_directory TEXT,
                    environment TEXT NOT NULL,
                    dll_overrides TEXT NOT NULL
                 );
                 INSERT INTO compatibility_defaults
                    (id, arguments_before, arguments_after, environment, dll_overrides)
                 VALUES (1, '[]', '[]', '{}', '{}');
                 CREATE TABLE game_compatibility_overrides (
                    game_id TEXT PRIMARY KEY NOT NULL REFERENCES games(id) ON DELETE CASCADE,
                    runner_path TEXT,
                    prefix_path TEXT,
                    arguments_before TEXT,
                    arguments_after TEXT,
                    working_directory TEXT,
                    environment TEXT,
                    dll_overrides TEXT
                 );
                 PRAGMA user_version = 11;",
            )
            .map_err(database_error)?;
    }
    if version < 12 {
        transaction
            .execute_batch(
                "CREATE TABLE game_sessions (
                    id INTEGER PRIMARY KEY,
                    game_id TEXT NOT NULL REFERENCES games(id) ON DELETE CASCADE,
                    started_at INTEGER NOT NULL,
                    last_seen_at INTEGER NOT NULL,
                    ended_at INTEGER,
                    end_reason TEXT CHECK (end_reason IN ('finished', 'interrupted')),
                    CHECK (last_seen_at >= started_at),
                    CHECK (ended_at IS NULL OR ended_at >= started_at)
                 );
                 CREATE INDEX game_sessions_game_id_started_at_idx
                    ON game_sessions (game_id, started_at);
                 PRAGMA user_version = 12;",
            )
            .map_err(database_error)?;
    }
    if version < 13 {
        transaction
            .execute_batch(
                "CREATE TABLE game_native_launch_config (
                game_id TEXT PRIMARY KEY NOT NULL REFERENCES games(id) ON DELETE CASCADE,
                arguments TEXT NOT NULL DEFAULT '[]',
                working_directory TEXT
             );
             PRAGMA user_version = 13;",
            )
            .map_err(database_error)?;
    }
    if version < 14 {
        transaction
            .execute_batch(
                r#"ALTER TABLE compatibility_defaults ADD COLUMN steam_runtime TEXT NOT NULL DEFAULT '"runner_default"';
                   ALTER TABLE compatibility_defaults ADD COLUMN steam_overlay TEXT NOT NULL DEFAULT '"runner_default"';
                   ALTER TABLE compatibility_defaults ADD COLUMN graphics_renderer TEXT NOT NULL DEFAULT '"runner_default"';
                   ALTER TABLE compatibility_defaults ADD COLUMN wayland TEXT NOT NULL DEFAULT '"runner_default"';
                   ALTER TABLE game_compatibility_overrides ADD COLUMN steam_runtime TEXT;
                   ALTER TABLE game_compatibility_overrides ADD COLUMN steam_overlay TEXT;
                   ALTER TABLE game_compatibility_overrides ADD COLUMN graphics_renderer TEXT;
                   ALTER TABLE game_compatibility_overrides ADD COLUMN wayland TEXT;
                   PRAGMA user_version = 14;"#,
            )
            .map_err(database_error)?;
    }
    if version < 15 {
        transaction
            .execute_batch(
                "ALTER TABLE compatibility_defaults
                    ADD COLUMN debug_logging INTEGER NOT NULL DEFAULT 0
                    CHECK (debug_logging IN (0, 1));
                 ALTER TABLE game_compatibility_overrides
                    ADD COLUMN debug_logging INTEGER
                    CHECK (debug_logging IS NULL OR debug_logging IN (0, 1));
                 PRAGMA user_version = 15;",
            )
            .map_err(database_error)?;
    }
    if version < 16 {
        transaction
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS game_steam_launch_config (
                    game_id TEXT PRIMARY KEY NOT NULL REFERENCES games(id) ON DELETE CASCADE,
                    arguments TEXT NOT NULL DEFAULT '[]'
                 );
                 CREATE TABLE steam_details_cache_v16 (
                    steam_app_id INTEGER PRIMARY KEY CHECK (steam_app_id BETWEEN 1 AND 4294967295),
                    details TEXT NOT NULL CHECK (length(CAST(details AS BLOB)) BETWEEN 1 AND 524288),
                    fetched_at INTEGER NOT NULL CHECK (fetched_at >= 0)
                 );
                 INSERT INTO steam_details_cache_v16 (steam_app_id, details, fetched_at)
                 SELECT steam_app_id, details, fetched_at FROM steam_details_cache;
                 DROP TABLE steam_details_cache;
                 ALTER TABLE steam_details_cache_v16 RENAME TO steam_details_cache;
                 PRAGMA user_version = 16;",
            )
            .map_err(database_error)?;
    }
    if version < 17 {
        let existing: i64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('game_compatibility_overrides') WHERE name = 'launch_via_steam'",
                [],
                |row| row.get(0),
            )
            .map_err(database_error)?;
        if existing == 0 {
            transaction.execute_batch(
                "ALTER TABLE game_compatibility_overrides ADD COLUMN launch_via_steam INTEGER CHECK (launch_via_steam IS NULL OR launch_via_steam IN (0, 1));"
            ).map_err(database_error)?;
        }
        transaction
            .execute_batch("PRAGMA user_version = 17;")
            .map_err(database_error)?;
    }
    if version < 18 {
        transaction
            .execute_batch(
                "ALTER TABLE compatibility_defaults DROP COLUMN steam_runtime;
                 ALTER TABLE compatibility_defaults DROP COLUMN steam_overlay;
                 ALTER TABLE game_compatibility_overrides DROP COLUMN steam_runtime;
                 ALTER TABLE game_compatibility_overrides DROP COLUMN steam_overlay;
                 PRAGMA user_version = 18;",
            )
            .map_err(database_error)?;
    }
    if version < 19 {
        let existing: i64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('game_compatibility_overrides') WHERE name = 'online_fix'",
                [],
                |row| row.get(0),
            )
            .map_err(database_error)?;
        if existing == 0 {
            transaction
                .execute_batch(
                    "ALTER TABLE game_compatibility_overrides
                        ADD COLUMN online_fix INTEGER
                        CHECK (online_fix IS NULL OR online_fix IN (0, 1));",
                )
                .map_err(database_error)?;
        }
        transaction
            .execute_batch("PRAGMA user_version = 19;")
            .map_err(database_error)?;
    }
    if version < 20 {
        let existing: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('downloads') WHERE name = 'source_verified'",
            [], |row| row.get(0),
        ).map_err(database_error)?;
        if existing == 0 {
            transaction.execute_batch(
                "ALTER TABLE downloads ADD COLUMN source_verified INTEGER NOT NULL DEFAULT 1 CHECK (source_verified IN (0, 1));"
            ).map_err(database_error)?;
        }
        let bytes: Option<Vec<u8>> = transaction
            .query_row(
                "SELECT manifest FROM legio_source_cache WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(database_error)?;
        if let Some(bytes) = bytes {
            match crate::legio_source::parse_manifest(&bytes) {
                Ok(manifest) => {
                    for entry in manifest.unverified {
                        transaction.execute(
                            "UPDATE downloads SET source_verified = 0 WHERE steam_app_id = ?1 AND url = ?2 AND release_version = ?3",
                            params![entry.steam_app_id, entry.download.url, entry.release.version],
                        ).map_err(database_error)?;
                    }
                }
                Err(error) => crate::application_log::failure("classify_download_sources", error),
            }
        }
        transaction
            .execute_batch("PRAGMA user_version = 20;")
            .map_err(database_error)?;
    }
    if version < 21 {
        let existing: i64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('downloads') WHERE name = 'queue_position'",
                [],
                |row| row.get(0),
            )
            .map_err(database_error)?;
        if existing == 0 {
            transaction
                .execute_batch(
                    "ALTER TABLE downloads ADD COLUMN queue_position INTEGER NOT NULL DEFAULT 0;",
                )
                .map_err(database_error)?;
        }
        transaction
            .execute_batch("PRAGMA user_version = 21;")
            .map_err(database_error)?;
    }
    if version < 22 {
        let existing: i64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('games') WHERE name = 'installation_root'",
                [],
                |row| row.get(0),
            )
            .map_err(database_error)?;
        if existing == 0 {
            transaction
                .execute_batch("ALTER TABLE games ADD COLUMN installation_root TEXT;")
                .map_err(database_error)?;
        }
        let localized: i64 = transaction.query_row("SELECT COUNT(*) FROM pragma_table_info('steam_details_cache') WHERE name = 'language'", [], |row| row.get(0)).map_err(database_error)?;
        if localized == 0 {
            transaction.execute_batch(
                "ALTER TABLE steam_details_cache RENAME TO steam_details_legacy;
                 CREATE TABLE steam_details_cache (steam_app_id INTEGER NOT NULL, details TEXT NOT NULL, fetched_at INTEGER NOT NULL, language TEXT NOT NULL DEFAULT 'en' CHECK (language IN ('it', 'en')), PRIMARY KEY (steam_app_id, language));
                 INSERT INTO steam_details_cache (steam_app_id, details, fetched_at) SELECT steam_app_id, details, fetched_at FROM steam_details_legacy;
                 DROP TABLE steam_details_legacy;"
            ).map_err(database_error)?;
        }
        transaction.execute_batch(
            "UPDATE games SET installation_root = (SELECT final_path FROM downloads WHERE downloads.id = games.id AND status = 'installed') WHERE installation_root IS NULL;
             CREATE TABLE IF NOT EXISTS game_transfers (
                game_id TEXT PRIMARY KEY REFERENCES games(id) ON DELETE RESTRICT,
                source TEXT NOT NULL, destination TEXT NOT NULL, temporary TEXT NOT NULL,
                executable_relative TEXT NOT NULL,
                phase TEXT NOT NULL CHECK (phase IN ('prepared', 'copied', 'committed'))
             );
             PRAGMA user_version = 22;"
        ).map_err(database_error)?;
    }
    if version < 23 {
        let existing: i64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('game_native_launch_config') WHERE name = 'environment'",
                [],
                |row| row.get(0),
            )
            .map_err(database_error)?;
        if existing == 0 {
            transaction
                .execute_batch("ALTER TABLE game_native_launch_config ADD COLUMN environment TEXT NOT NULL DEFAULT '{}';")
                .map_err(database_error)?;
        }
        transaction
            .execute_batch("PRAGMA user_version = 23;")
            .map_err(database_error)?;
    }
    if version < 24 {
        let existing: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('game_compatibility_overrides') WHERE name = 'linux_performance'",
            [], |row| row.get(0),
        ).map_err(database_error)?;
        if existing == 0 {
            transaction
                .execute_batch(
                    "ALTER TABLE game_compatibility_overrides ADD COLUMN linux_performance TEXT;",
                )
                .map_err(database_error)?;
        }
        transaction
            .execute_batch("PRAGMA user_version = 24;")
            .map_err(database_error)?;
    }
    if version < 25 {
        transaction
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS download_sources (
                id TEXT PRIMARY KEY,
                url TEXT NOT NULL UNIQUE,
                manifest BLOB NOT NULL CHECK (length(manifest) BETWEEN 1 AND 2097152),
                fetched_at INTEGER NOT NULL CHECK (fetched_at >= 0),
                warning TEXT
             );
             DELETE FROM legio_source_cache;
             PRAGMA user_version = 25;",
            )
            .map_err(database_error)?;
    }
    if version < 26 {
        for (name, definition) in [
            ("source_id", "TEXT"),
            ("source_entry_id", "TEXT"),
            ("target_game_id", "TEXT"),
            ("pause_reason", "TEXT"),
            ("manual_override", "INTEGER NOT NULL DEFAULT 0"),
            ("automatic_update", "INTEGER NOT NULL DEFAULT 0"),
            ("interrupted_by_game", "INTEGER NOT NULL DEFAULT 0"),
        ] {
            let exists: bool = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM pragma_table_info('downloads') WHERE name = ?1)",
                    [name],
                    |row| row.get(0),
                )
                .map_err(database_error)?;
            if !exists {
                transaction
                    .execute_batch(&format!(
                        "ALTER TABLE downloads ADD COLUMN {name} {definition}"
                    ))
                    .map_err(database_error)?;
            }
        }
        transaction
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS game_source_installs (
                game_id TEXT PRIMARY KEY REFERENCES games(id) ON DELETE CASCADE,
                source_id TEXT NOT NULL, entry_id TEXT,
                installed_version TEXT NOT NULL, accepted_unverified INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS game_update_journals (
                job_id TEXT PRIMARY KEY REFERENCES downloads(id) ON DELETE RESTRICT,
                game_id TEXT NOT NULL REFERENCES games(id) ON DELETE RESTRICT,
                target TEXT NOT NULL, backup TEXT NOT NULL, manifest TEXT NOT NULL,
                committed INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS downloads_update_target ON downloads(target_game_id, status);
            CREATE UNIQUE INDEX IF NOT EXISTS downloads_active_update ON downloads(target_game_id) WHERE target_game_id IS NOT NULL AND status NOT IN ('installed','cancelled');
            PRAGMA user_version = 26;",
            )
            .map_err(database_error)?;
    }
    if version < 27 {
        for (table, column, definition) in [
            ("games", "archive_layout", "TEXT NOT NULL DEFAULT 'direct'"),
            ("games", "playtime_adjustment", "INTEGER NOT NULL DEFAULT 0"),
            ("download_sources", "url_sha256", "TEXT"),
            ("game_source_installs", "source_sha256", "TEXT"),
        ] {
            let exists: bool = transaction
                .query_row(
                    &format!(
                        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('{table}') WHERE name = ?1)"
                    ),
                    [column],
                    |row| row.get(0),
                )
                .map_err(database_error)?;
            if !exists {
                transaction
                    .execute_batch(&format!(
                        "ALTER TABLE {table} ADD COLUMN {column} {definition};"
                    ))
                    .map_err(database_error)?;
            }
        }
        let installs: Vec<(String, String)> = {
            let mut q = transaction
                .prepare(
                    "SELECT id, installation_root FROM games WHERE installation_root IS NOT NULL",
                )
                .map_err(database_error)?;
            q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(database_error)?
                .collect::<Result<_, _>>()
                .map_err(database_error)?
        };
        for (id, root) in installs {
            if Path::new(&root)
                .file_name()
                .is_some_and(|name| name == id.as_str())
            {
                transaction
                    .execute(
                        "UPDATE games SET archive_layout = 'legacy' WHERE id = ?1",
                        [&id],
                    )
                    .map_err(database_error)?;
            }
        }
        let sources: Vec<(String, String)> = {
            let mut q = transaction
                .prepare("SELECT id, url FROM download_sources")
                .map_err(database_error)?;
            q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(database_error)?
                .collect::<Result<_, _>>()
                .map_err(database_error)?
        };
        for (id, url) in sources {
            let hash = crate::game_library::source_hash(&url)?;
            transaction
                .execute(
                    "UPDATE download_sources SET url_sha256 = ?2 WHERE id = ?1",
                    params![id, hash],
                )
                .map_err(database_error)?;
        }
        transaction.execute_batch("CREATE UNIQUE INDEX IF NOT EXISTS download_sources_hash ON download_sources(url_sha256);
            UPDATE game_source_installs SET source_sha256 = (SELECT url_sha256 FROM download_sources s WHERE s.id = source_id) WHERE source_sha256 IS NULL;
            CREATE TABLE IF NOT EXISTS game_libraries (id TEXT PRIMARY KEY, root TEXT NOT NULL UNIQUE, dirty INTEGER NOT NULL DEFAULT 1, warning TEXT, manifest_hash TEXT);
            PRAGMA user_version = 27;").map_err(database_error)?;
        for event in ["INSERT", "UPDATE", "DELETE"] {
            let records: &[&str] = match event {
                "INSERT" => &["NEW"],
                "DELETE" => &["OLD"],
                _ => &["OLD", "NEW"],
            };
            let roots = records.iter().map(|record|format!("substr(replace({record}.installation_root, char(92), '/'),1,length(replace(root,char(92),'/'))+1)=replace(root,char(92),'/')||'/'")).collect::<Vec<_>>().join(" OR ");
            transaction.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS library_dirty_games_{event} AFTER {event} ON games BEGIN UPDATE game_libraries SET dirty=dirty+1 WHERE {roots}; END;")).map_err(database_error)?;
            let games = records
                .iter()
                .map(|record| format!("g.id={record}.game_id"))
                .collect::<Vec<_>>()
                .join(" OR ");
            transaction.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS library_dirty_game_source_installs_{event} AFTER {event} ON game_source_installs BEGIN UPDATE game_libraries SET dirty=dirty+1 WHERE EXISTS(SELECT 1 FROM games g WHERE ({games}) AND substr(replace(g.installation_root,char(92),'/'),1,length(replace(root,char(92),'/'))+1)=replace(root,char(92),'/')||'/'); END;")).map_err(database_error)?;
        }
    }
    if version < 28 {
        for (table, columns) in [
            (
                "games",
                &[
                    "automatic_name",
                    "name_override",
                    "installation_root",
                    "executable_path",
                    "steam_app_id",
                    "steam_install_path",
                    "archive_layout",
                ][..],
            ),
            (
                "game_source_installs",
                &[
                    "game_id",
                    "source_id",
                    "source_sha256",
                    "entry_id",
                    "installed_version",
                ][..],
            ),
        ] {
            let changed = columns
                .iter()
                .map(|name| format!("OLD.{name} IS NOT NEW.{name}"))
                .collect::<Vec<_>>()
                .join(" OR ");
            let affected = if table == "games" {
                "substr(replace(OLD.installation_root,char(92),'/'),1,length(replace(root,char(92),'/'))+1)=replace(root,char(92),'/')||'/' OR substr(replace(NEW.installation_root,char(92),'/'),1,length(replace(root,char(92),'/'))+1)=replace(root,char(92),'/')||'/'".to_owned()
            } else {
                "EXISTS(SELECT 1 FROM games g WHERE (g.id=OLD.game_id OR g.id=NEW.game_id) AND substr(replace(g.installation_root,char(92),'/'),1,length(replace(root,char(92),'/'))+1)=replace(root,char(92),'/')||'/')".to_owned()
            };
            transaction.execute_batch(&format!("DROP TRIGGER IF EXISTS library_dirty_{table}_UPDATE; CREATE TRIGGER library_dirty_{table}_UPDATE AFTER UPDATE OF {} ON {table} WHEN {changed} BEGIN UPDATE game_libraries SET dirty=dirty+1 WHERE {affected}; END;",columns.join(","))).map_err(database_error)?;
        }
        transaction
            .pragma_update(None, "user_version", 28)
            .map_err(database_error)?;
    }
    if version < 29 {
        transaction
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS hidden_games (
                game_id TEXT PRIMARY KEY NOT NULL REFERENCES games(id) ON DELETE CASCADE
             );
             PRAGMA user_version = 29;",
            )
            .map_err(database_error)?;
    }
    if version < 30 {
        // Source caches now use a format-aware envelope.
        transaction
            .execute_batch("PRAGMA user_version = 30;")
            .map_err(database_error)?;
    }
    if version < 31 {
        let exists: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('downloads') WHERE name='mirror_plan')", [], |row| row.get(0)).map_err(database_error)?;
        if !exists {
            transaction
                .execute_batch("ALTER TABLE downloads ADD COLUMN mirror_plan TEXT;")
                .map_err(database_error)?;
        }
        transaction
            .pragma_update(None, "user_version", 31)
            .map_err(database_error)?;
    }
    if version < 32 {
        let exists: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('downloads') WHERE name='published_size_bytes')", [], |row| row.get(0)).map_err(database_error)?;
        if !exists {
            transaction
                .execute_batch(
                    "ALTER TABLE downloads ADD COLUMN published_size_bytes INTEGER;
                UPDATE downloads SET published_size_bytes=size_bytes WHERE mirror_plan IS NULL;",
                )
                .map_err(database_error)?;
        }
        transaction
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS game_uninstalls (game_id TEXT PRIMARY KEY NOT NULL REFERENCES games(id) ON DELETE CASCADE, installation_root TEXT NOT NULL);
            PRAGMA user_version = 32;",
            )
            .map_err(database_error)?;
    }
    if version < 33 {
        transaction
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS link_checks (
                url TEXT PRIMARY KEY NOT NULL,
                size_bytes INTEGER,
                error TEXT,
                checked_at INTEGER NOT NULL
             );
             PRAGMA user_version = 33;",
            )
            .map_err(database_error)?;
    }
    if version < 34 {
        transaction
            .execute_batch(
                "ALTER TABLE game_transfers RENAME TO game_transfers_legacy;
             CREATE TABLE game_transfers (
                game_id TEXT PRIMARY KEY REFERENCES games(id) ON DELETE RESTRICT,
                source TEXT NOT NULL, destination TEXT NOT NULL, temporary TEXT NOT NULL,
                executable_relative TEXT NOT NULL,
                phase TEXT NOT NULL CHECK (phase IN ('prepared', 'moving', 'copied', 'committed'))
             );
             INSERT INTO game_transfers (game_id, source, destination, temporary, executable_relative, phase)
                SELECT game_id, source, destination, temporary, executable_relative, phase FROM game_transfers_legacy;
             DROP TABLE game_transfers_legacy;
             PRAGMA user_version = 34;",
            )
            .map_err(database_error)?;
    }
    if version < 35 {
        let exists: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('downloads') WHERE name='library_id')",
                [],
                |row| row.get(0),
            )
            .map_err(database_error)?;
        if !exists {
            transaction
                .execute_batch("ALTER TABLE downloads ADD COLUMN library_id TEXT;")
                .map_err(database_error)?;
        }
        transaction
            .pragma_update(None, "user_version", 35)
            .map_err(database_error)?;
    }
    transaction.commit().map_err(database_error)
}
