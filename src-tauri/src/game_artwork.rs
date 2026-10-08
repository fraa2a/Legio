use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use serde::Serialize;

use crate::{database::Game, image_format::ImageFormat};

const MAX_ARTWORK_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ArtworkKind {
    Icon,
    Banner,
    ShortcutIcon,
}

impl ArtworkKind {
    fn directory(self) -> &'static str {
        match self {
            Self::Icon => "game-icons",
            Self::Banner => "game-banners",
            Self::ShortcutIcon => "game-shortcut-icons",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Icon => "game icon",
            Self::Banner => "game banner",
            Self::ShortcutIcon => "game shortcut icon",
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GameArtworkResult {
    #[serde(skip_serializing)]
    bytes: Vec<u8>,
    content_type: &'static str,
}

impl GameArtworkResult {
    pub(crate) fn into_response(self) -> Result<tauri::ipc::Response, String> {
        let metadata = serde_json::to_vec(&self).map_err(|error| error.to_string())?;
        crate::image_response::encode(metadata, self.bytes)
    }
}

#[derive(Clone)]
pub(crate) struct GameArtworkStore {
    app_data_dir: Result<PathBuf, String>,
    lock: Arc<Mutex<()>>,
}

struct StoredArtwork {
    bytes: Vec<u8>,
    format: ImageFormat,
}

impl GameArtworkStore {
    pub(crate) fn new(app_data_dir: Result<PathBuf, String>) -> Self {
        Self {
            app_data_dir,
            lock: Arc::new(Mutex::new(())),
        }
    }

    pub(crate) fn set(
        &self,
        game_id: &str,
        kind: ArtworkKind,
        source: &Path,
    ) -> Result<GameArtworkResult, String> {
        let game_id = canonical_game_id(game_id)?;
        let bytes = read_image(source, &format!("selected {}", kind.label()))?;
        if ImageFormat::from_bytes(&bytes).is_some() {
            crate::image_trim::validate_image(&bytes)?;
        }
        self.store(game_id, kind, bytes)
    }

    pub(crate) fn shortcut_icon_path(&self, game: &Game) -> Result<Option<PathBuf>, String> {
        self.path(&game.id, ArtworkKind::Icon)
    }

    fn store(
        &self,
        game_id: String,
        kind: ArtworkKind,
        bytes: Vec<u8>,
    ) -> Result<GameArtworkResult, String> {
        if bytes.len() > MAX_ARTWORK_BYTES {
            return Err(format!("The {} exceeds the 2 MiB size limit", kind.label()));
        }
        let format = ImageFormat::from_bytes(&bytes).ok_or_else(|| {
            format!(
                "The {} is not a supported PNG, JPEG, or WebP image",
                kind.label()
            )
        })?;
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Game artwork store lock was poisoned".to_owned())?;
        let directory = self.ensure_directory(kind)?;
        let existing = existing_paths(&directory, &game_id, kind)?;
        let destination = artwork_path(&directory, &game_id, format);
        write_artwork(&directory, &destination, &game_id, kind, &bytes)?;
        for path in existing {
            if path != destination {
                fs::remove_file(path).map_err(|error| {
                    format!("Could not replace the previous {}: {error}", kind.label())
                })?;
            }
        }
        Ok(GameArtworkResult {
            bytes,
            content_type: format.content_type(),
        })
    }

    pub(crate) fn get(
        &self,
        game_id: &str,
        kind: ArtworkKind,
    ) -> Result<Option<GameArtworkResult>, String> {
        let game_id = canonical_game_id(game_id)?;
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Game artwork store lock was poisoned".to_owned())?;
        let directory = self.directory(kind)?;
        Ok(
            load_artwork(&directory, &game_id, kind)?.map(|artwork| GameArtworkResult {
                bytes: artwork.bytes,
                content_type: artwork.format.content_type(),
            }),
        )
    }

    pub(crate) fn path(&self, game_id: &str, kind: ArtworkKind) -> Result<Option<PathBuf>, String> {
        let game_id = canonical_game_id(game_id)?;
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Game artwork store lock was poisoned".to_owned())?;
        let directory = self.directory(kind)?;
        let paths = existing_paths(&directory, &game_id, kind)?;
        if paths.len() > 1 {
            return Err(format!(
                "More than one custom {} is stored for this game",
                kind.label()
            ));
        }
        let Some(path) = paths.into_iter().next() else {
            return Ok(None);
        };
        let mut header = [0; 12];
        let file = open_image(&path, &format!("stored {}", kind.label()))?;
        let count = file
            .take(header.len() as u64)
            .read(&mut header)
            .map_err(|error| error.to_string())?;
        let format = ImageFormat::from_bytes(&header[..count])
            .ok_or("Stored artwork has an unsupported image format")?;
        if artwork_path(&directory, &game_id, format) != path {
            return Err(format!(
                "Stored {} format does not match its file name",
                kind.label()
            ));
        }
        Ok(Some(path))
    }

    pub(crate) fn remove(&self, game_id: &str, kind: ArtworkKind) -> Result<(), String> {
        let game_id = canonical_game_id(game_id)?;
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Game artwork store lock was poisoned".to_owned())?;
        self.remove_locked(&game_id, kind)
    }

    pub(crate) fn remove_for_game(&self, game_id: &str) -> Result<(), String> {
        let game_id = canonical_game_id(game_id)?;
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Game artwork store lock was poisoned".to_owned())?;
        let mut errors = Vec::new();
        for kind in [
            ArtworkKind::Icon,
            ArtworkKind::Banner,
            ArtworkKind::ShortcutIcon,
        ] {
            if let Err(error) = self.remove_locked(&game_id, kind) {
                errors.push(error);
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    fn remove_locked(&self, game_id: &str, kind: ArtworkKind) -> Result<(), String> {
        let directory = self.directory(kind)?;
        for path in existing_paths(&directory, game_id, kind)? {
            fs::remove_file(path)
                .map_err(|error| format!("Could not remove the {}: {error}", kind.label()))?;
        }
        Ok(())
    }

    fn directory(&self, kind: ArtworkKind) -> Result<PathBuf, String> {
        self.app_data_dir
            .as_deref()
            .map(|path| path.join(kind.directory()))
            .map_err(Clone::clone)
    }

    fn ensure_directory(&self, kind: ArtworkKind) -> Result<PathBuf, String> {
        let directory = self.directory(kind)?;
        fs::create_dir_all(&directory)
            .map_err(|error| format!("Could not create the {} directory: {error}", kind.label()))?;
        let metadata = fs::symlink_metadata(&directory).map_err(|error| {
            format!("Could not inspect the {} directory: {error}", kind.label())
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(format!(
                "The {} directory is not a regular directory",
                kind.label()
            ));
        }
        Ok(directory)
    }
}

fn canonical_game_id(game_id: &str) -> Result<String, String> {
    uuid::Uuid::parse_str(game_id)
        .map(|id| id.to_string())
        .map_err(|_| "Could not access game artwork: invalid game ID".to_owned())
}

fn existing_paths(
    directory: &Path,
    game_id: &str,
    kind: ArtworkKind,
) -> Result<Vec<PathBuf>, String> {
    let metadata = match fs::symlink_metadata(directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(format!(
                "Could not inspect the {} directory: {error}",
                kind.label()
            ));
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "The {} directory is not a regular directory",
            kind.label()
        ));
    }

    let mut paths = Vec::new();
    for format in ImageFormat::ALL {
        let path = artwork_path(directory, game_id, format);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(format!("A {} path is not a regular file", kind.label()));
            }
            Ok(_) => paths.push(path),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("Could not inspect a {}: {error}", kind.label())),
        }
    }
    Ok(paths)
}

fn load_artwork(
    directory: &Path,
    game_id: &str,
    kind: ArtworkKind,
) -> Result<Option<StoredArtwork>, String> {
    let paths = existing_paths(directory, game_id, kind)?;
    if paths.len() > 1 {
        return Err(format!(
            "More than one custom {} is stored for this game",
            kind.label()
        ));
    }
    let Some(path) = paths.into_iter().next() else {
        return Ok(None);
    };
    let bytes = read_image(&path, &format!("stored {}", kind.label()))?;
    let format = ImageFormat::from_bytes(&bytes)
        .ok_or_else(|| format!("Stored {} has an unsupported image format", kind.label()))?;
    if artwork_path(directory, game_id, format) != path {
        return Err(format!(
            "Stored {} format does not match its file name",
            kind.label()
        ));
    }
    Ok(Some(StoredArtwork { bytes, format }))
}

fn artwork_path(directory: &Path, game_id: &str, format: ImageFormat) -> PathBuf {
    directory.join(format!("{game_id}.{}", format.extension()))
}

fn read_image(path: &Path, label: &str) -> Result<Vec<u8>, String> {
    read_bounded(open_image(path, label)?, label)
}

fn open_image(path: &Path, label: &str) -> Result<File, String> {
    if !path.is_absolute() {
        return Err(format!("The {label} path must be absolute"));
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Could not inspect {label}: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("The {label} is not a regular file"));
    }
    if metadata.len() > MAX_ARTWORK_BYTES as u64 {
        return Err(format!("The {label} exceeds the 2 MiB size limit"));
    }

    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options
        .open(path)
        .map_err(|error| format!("Could not open {label}: {error}"))?;
    let opened = file
        .metadata()
        .map_err(|error| format!("Could not inspect {label}: {error}"))?;
    if !opened.is_file() || opened.len() > MAX_ARTWORK_BYTES as u64 {
        return Err(format!(
            "The {label} is not a regular image within the size limit"
        ));
    }
    Ok(file)
}

fn read_bounded(file: File, label: &str) -> Result<Vec<u8>, String> {
    if !file
        .metadata()
        .map_err(|error| format!("Could not inspect {label}: {error}"))?
        .is_file()
    {
        return Err(format!("The {label} is not a regular file"));
    }
    let mut bytes = Vec::new();
    file.take((MAX_ARTWORK_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read {label}: {error}"))?;
    if bytes.len() > MAX_ARTWORK_BYTES {
        return Err(format!("The {label} exceeds the 2 MiB size limit"));
    }
    Ok(bytes)
}

fn write_artwork(
    directory: &Path,
    destination: &Path,
    game_id: &str,
    kind: ArtworkKind,
    bytes: &[u8],
) -> Result<(), String> {
    let temporary = directory.join(format!(".{game_id}.{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|error| format!("Could not create a temporary {}: {error}", kind.label()))?;
        file.write_all(bytes)
            .map_err(|error| format!("Could not write the {}: {error}", kind.label()))?;
        file.sync_all()
            .map_err(|error| format!("Could not flush the {}: {error}", kind.label()))?;
        fs::rename(&temporary, destination)
            .map_err(|error| format!("Could not install the {}: {error}", kind.label()))?;
        Ok(())
    })();
    if let Err(error) = result {
        if let Err(cleanup_error) = fs::remove_file(&temporary)
            && cleanup_error.kind() != io::ErrorKind::NotFound
        {
            return Err(format!(
                "{error}; could not remove temporary {}: {cleanup_error}",
                kind.label()
            ));
        }
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    const PNG: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 4,
        0, 0, 0, 181, 28, 12, 2, 0, 0, 0, 11, 73, 68, 65, 84, 120, 218, 99, 100, 248, 15, 0, 1, 5,
        1, 1, 39, 24, 227, 102, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    fn jpeg_image() -> Vec<u8> {
        let mut output = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(1, 1, image::Rgb([20, 40, 60])))
            .write_to(&mut output, image::ImageFormat::Jpeg)
            .unwrap();
        output.into_inner()
    }

    fn test_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!("legio-game-icons-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn custom_artwork_rejects_compressed_images_with_excessive_dimensions() {
        let root = test_dir();
        let store = GameArtworkStore::new(Ok(root.clone()));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::new(8193, 1))
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let source = root.join("oversized.png");
        fs::write(&source, bytes.into_inner()).unwrap();
        assert!(
            store
                .set(
                    "00000000-0000-0000-0000-000000000001",
                    ArtworkKind::Icon,
                    &source
                )
                .unwrap_err()
                .contains("dimension")
        );
        fs::remove_dir_all(root).unwrap();
    }

    fn manual_game() -> Game {
        Game {
            installation_root: None,
            id: "00000000-0000-0000-0000-000000000001".to_owned(),
            steam_app_id: None,
            automatic_name: None,
            name_override: None,
            name: "Imported game".to_owned(),
            steam_install_path: None,
            steam_account_id: None,
            executable_path: None,
        }
    }

    #[test]
    fn shortcut_uses_custom_icon_without_reading_the_executable() {
        let root = test_dir();
        let source = root.join("selected.png");
        fs::write(&source, PNG).unwrap();
        let store = GameArtworkStore::new(Ok(root.join("app-data")));
        let mut game = manual_game();
        game.executable_path = Some(root.join("invalid.exe").to_string_lossy().into_owned());
        fs::write(game.executable_path.as_ref().unwrap(), b"MZ").unwrap();

        assert_eq!(store.shortcut_icon_path(&game).unwrap(), None);
        assert!(!root.join("app-data/game-shortcut-icons").exists());
        store.set(&game.id, ArtworkKind::Icon, &source).unwrap();
        assert_eq!(
            store.shortcut_icon_path(&game).unwrap(),
            store.path(&game.id, ArtworkKind::Icon).unwrap()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn icon_is_persisted_and_returned_with_its_content_type() {
        let root = test_dir();
        let source = root.join("selected.png");
        let app_data = root.join("app-data");
        fs::write(&source, PNG).unwrap();
        let store = GameArtworkStore::new(Ok(app_data.clone()));
        let icon = store
            .set(
                "00000000-0000-0000-0000-000000000001",
                ArtworkKind::Icon,
                &source,
            )
            .unwrap();
        assert_eq!(icon.content_type, "image/png");

        let reopened = GameArtworkStore::new(Ok(app_data));
        assert_eq!(
            reopened
                .get("00000000-0000-0000-0000-000000000001", ArtworkKind::Icon)
                .unwrap()
                .unwrap()
                .bytes,
            PNG
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unsupported_images_are_rejected_without_creating_storage() {
        let root = test_dir();
        let source = root.join("selected.exe");
        fs::write(&source, b"MZ").unwrap();
        let store = GameArtworkStore::new(Ok(root.join("app-data")));

        let error = store
            .set(
                "00000000-0000-0000-0000-000000000001",
                ArtworkKind::Icon,
                &source,
            )
            .unwrap_err();
        assert!(error.contains("PNG, JPEG, or WebP"));
        assert!(!root.join("app-data/game-icons").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn oversized_icon_is_rejected_before_it_is_stored() {
        let root = test_dir();
        let source = root.join("large.png");
        fs::write(&source, vec![0; MAX_ARTWORK_BYTES + 1]).unwrap();
        let store = GameArtworkStore::new(Ok(root.join("app-data")));

        let error = store
            .set(
                "00000000-0000-0000-0000-000000000001",
                ArtworkKind::Icon,
                &source,
            )
            .unwrap_err();
        assert!(error.contains("2 MiB size limit"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reset_removes_the_icon_and_repeated_reset_succeeds() {
        let root = test_dir();
        let source = root.join("selected.png");
        fs::write(&source, PNG).unwrap();
        let store = GameArtworkStore::new(Ok(root.join("app-data")));
        let game_id = "00000000-0000-0000-0000-000000000001";
        store.set(game_id, ArtworkKind::Icon, &source).unwrap();

        store.remove(game_id, ArtworkKind::Icon).unwrap();
        assert!(store.get(game_id, ArtworkKind::Icon).unwrap().is_none());
        store.remove(game_id, ArtworkKind::Icon).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn replacing_an_icon_updates_its_format_and_removes_the_old_file() {
        let root = test_dir();
        let png = root.join("selected.png");
        let jpeg = root.join("selected.jpg");
        let app_data = root.join("app-data");
        let game_id = "00000000-0000-0000-0000-000000000001";
        fs::write(&png, PNG).unwrap();
        fs::write(&jpeg, jpeg_image()).unwrap();
        let store = GameArtworkStore::new(Ok(app_data.clone()));
        store.set(game_id, ArtworkKind::Icon, &png).unwrap();

        let replaced = store.set(game_id, ArtworkKind::Icon, &jpeg).unwrap();

        assert_eq!(replaced.content_type, "image/jpeg");
        assert!(!app_data.join(format!("game-icons/{game_id}.png")).exists());
        assert!(app_data.join(format!("game-icons/{game_id}.jpg")).is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn custom_artwork_paths_are_confined_to_the_game_id_and_format() {
        let root = test_dir();
        let source = root.join("selected.png");
        fs::write(&source, PNG).unwrap();
        let store = GameArtworkStore::new(Ok(root.join("app-data")));
        let path = store
            .set(
                "00000000-0000-0000-0000-000000000001",
                ArtworkKind::Icon,
                &source,
            )
            .and_then(|_| store.path("00000000-0000-0000-0000-000000000001", ArtworkKind::Icon))
            .unwrap()
            .unwrap();
        assert_eq!(
            path.file_name().unwrap(),
            "00000000-0000-0000-0000-000000000001.png"
        );
        assert!(store.path("../../etc/passwd", ArtworkKind::Icon).is_err());
        assert!(
            store
                .set("../../etc/passwd", ArtworkKind::Banner, &source)
                .unwrap_err()
                .contains("invalid game ID")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn selected_icon_symlinks_are_rejected() {
        let root = test_dir();
        let target = root.join("icon.png");
        let source = root.join("selected.png");
        fs::write(&target, PNG).unwrap();
        symlink(&target, &source).unwrap();
        let store = GameArtworkStore::new(Ok(root.join("app-data")));
        assert!(
            store
                .set(
                    "00000000-0000-0000-0000-000000000001",
                    ArtworkKind::Icon,
                    &source,
                )
                .is_err()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn banner_is_persisted_separately_from_the_icon() {
        let root = test_dir();
        let icon_source = root.join("icon.png");
        let banner_source = root.join("banner.jpg");
        let app_data = root.join("app-data");
        let game_id = "00000000-0000-0000-0000-000000000001";
        fs::write(&icon_source, PNG).unwrap();
        fs::write(&banner_source, jpeg_image()).unwrap();
        let store = GameArtworkStore::new(Ok(app_data.clone()));
        store.set(game_id, ArtworkKind::Icon, &icon_source).unwrap();
        let banner = store
            .set(game_id, ArtworkKind::Banner, &banner_source)
            .unwrap();

        assert_eq!(banner.content_type, "image/jpeg");
        assert_eq!(
            app_data.join(format!("game-banners/{game_id}.jpg")),
            store.path(game_id, ArtworkKind::Banner).unwrap().unwrap()
        );
        let reopened = GameArtworkStore::new(Ok(app_data));
        assert_eq!(
            reopened
                .get(game_id, ArtworkKind::Banner)
                .unwrap()
                .unwrap()
                .bytes,
            jpeg_image()
        );
        assert!(reopened.get(game_id, ArtworkKind::Icon).unwrap().is_some());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reset_removes_only_the_custom_banner_and_is_idempotent() {
        let root = test_dir();
        let source = root.join("banner.png");
        let game_id = "00000000-0000-0000-0000-000000000001";
        fs::write(&source, PNG).unwrap();
        let store = GameArtworkStore::new(Ok(root.join("app-data")));
        store.set(game_id, ArtworkKind::Banner, &source).unwrap();

        store.remove(game_id, ArtworkKind::Banner).unwrap();
        assert!(store.get(game_id, ArtworkKind::Banner).unwrap().is_none());
        store.remove(game_id, ArtworkKind::Banner).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn removing_game_artwork_cleans_both_owned_files() {
        let root = test_dir();
        let source = root.join("selected.png");
        let app_data = root.join("app-data");
        let game_id = "00000000-0000-0000-0000-000000000001";
        fs::write(&source, PNG).unwrap();
        let store = GameArtworkStore::new(Ok(app_data.clone()));
        store.set(game_id, ArtworkKind::Icon, &source).unwrap();
        store.set(game_id, ArtworkKind::Banner, &source).unwrap();

        store.remove_for_game(game_id).unwrap();

        assert!(store.get(game_id, ArtworkKind::Icon).unwrap().is_none());
        assert!(store.get(game_id, ArtworkKind::Banner).unwrap().is_none());
        assert!(app_data.join("game-icons").is_dir());
        assert!(app_data.join("game-banners").is_dir());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn removing_game_artwork_reports_symlink_cleanup_failure_and_preserves_target() {
        let root = test_dir();
        let icon_source = root.join("icon.png");
        let target = root.join("outside.png");
        let banner_directory = root.join("app-data/game-banners");
        let game_id = "00000000-0000-0000-0000-000000000001";
        fs::write(&icon_source, PNG).unwrap();
        fs::write(&target, jpeg_image()).unwrap();
        let store = GameArtworkStore::new(Ok(root.join("app-data")));
        store.set(game_id, ArtworkKind::Icon, &icon_source).unwrap();
        fs::create_dir_all(&banner_directory).unwrap();
        symlink(&target, banner_directory.join(format!("{game_id}.jpg"))).unwrap();

        let error = store.remove_for_game(game_id).unwrap_err();

        assert!(error.contains("game banner path is not a regular file"));
        assert!(store.get(game_id, ArtworkKind::Icon).unwrap().is_none());
        assert_eq!(fs::read(target).unwrap(), jpeg_image());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn reset_refuses_a_symlink_in_the_managed_icon_directory() {
        let root = test_dir();
        let target = root.join("outside.png");
        let directory = root.join("app-data/game-icons");
        let game_id = "00000000-0000-0000-0000-000000000001";
        fs::write(&target, PNG).unwrap();
        fs::create_dir_all(&directory).unwrap();
        symlink(&target, directory.join(format!("{game_id}.png"))).unwrap();
        let store = GameArtworkStore::new(Ok(root.join("app-data")));

        assert!(store.remove(game_id, ArtworkKind::Icon).is_err());
        assert_eq!(fs::read(target).unwrap(), PNG);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn relative_artwork_paths_are_rejected() {
        assert!(
            read_image(Path::new("icon.png"), "selected icon")
                .unwrap_err()
                .contains("must be absolute")
        );
    }

    #[test]
    fn corrupt_stored_icons_report_an_error_and_can_be_reset() {
        let root = test_dir();
        let source = root.join("selected.png");
        fs::write(&source, PNG).unwrap();
        let store = GameArtworkStore::new(Ok(root.join("app-data")));
        let game_id = "00000000-0000-0000-0000-000000000001";
        store.set(game_id, ArtworkKind::Icon, &source).unwrap();
        fs::write(
            root.join(format!("app-data/game-icons/{game_id}.png")),
            b"invalid",
        )
        .unwrap();

        assert!(
            store
                .get(game_id, ArtworkKind::Icon)
                .unwrap_err()
                .contains("unsupported image format")
        );
        store.remove(game_id, ArtworkKind::Icon).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
