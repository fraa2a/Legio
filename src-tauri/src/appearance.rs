use std::{
    fs::{self, File, OpenOptions},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::database::DatabaseState;

const MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnimatedBackground {
    #[default]
    None,
    Particles,
    Dither,
}

impl<'de> Deserialize<'de> for AnimatedBackground {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "particles" => Self::Particles,
            "dither" => Self::Dither,
            _ => Self::None,
        })
    }
}

fn default_animated_opacity() -> u8 {
    65
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct DitherSettings {
    pub wave_speed: f32,
    pub wave_frequency: f32,
    pub wave_amplitude: f32,
    pub wave_color: Option<String>,
    pub background_color: String,
    pub color_num: u8,
    pub pixel_size: u8,
    pub disable_animation: bool,
    pub enable_mouse_interaction: bool,
    pub mouse_radius: f32,
}

impl Default for DitherSettings {
    fn default() -> Self {
        Self {
            wave_speed: 0.05,
            wave_frequency: 3.0,
            wave_amplitude: 0.3,
            wave_color: None,
            background_color: "#000000".to_owned(),
            color_num: 4,
            pixel_size: 2,
            disable_animation: false,
            enable_mouse_interaction: true,
            mouse_radius: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Appearance {
    pub background: Option<String>,
    pub background_blur: u8,
    pub background_opacity: u8,
    pub animated_background: AnimatedBackground,
    pub animated_opacity: u8,
    pub dither: DitherSettings,
    pub surface_opacity: u8,
    pub surface_blur: u8,
    pub dialog_opacity: u8,
    pub dialog_blur: u8,
    pub transparent: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            background: None,
            background_blur: 0,
            background_opacity: 60,
            animated_background: AnimatedBackground::None,
            animated_opacity: default_animated_opacity(),
            dither: DitherSettings::default(),
            surface_opacity: 90,
            surface_blur: 0,
            dialog_opacity: 90,
            dialog_blur: 0,
            transparent: false,
        }
    }
}

pub(crate) fn validate(appearance: &Appearance) -> Result<(), String> {
    if appearance.background_blur > 40
        || appearance.surface_blur > 40
        || appearance.dialog_blur > 40
        || appearance.background_opacity > 100
        || appearance.animated_opacity > 100
        || appearance.surface_opacity > 100
        || appearance.dialog_opacity > 100
    {
        return Err("Blur values must be 0-40 and opacity values must be 0-100".to_owned());
    }
    if let Some(id) = &appearance.background {
        validate_id(id)?;
    }
    validate_dither(&appearance.dither)
}

fn validate_dither(dither: &DitherSettings) -> Result<(), String> {
    let range = |value: f32, min: f32, max: f32| value.is_finite() && (min..=max).contains(&value);
    if !range(dither.wave_speed, 0.0, 1.0) {
        return Err("Dither wave speed must be between 0 and 1".to_owned());
    }
    if !range(dither.wave_frequency, 0.0, 10.0) {
        return Err("Dither wave frequency must be between 0 and 10".to_owned());
    }
    if !range(dither.wave_amplitude, 0.0, 1.0) {
        return Err("Dither wave amplitude must be between 0 and 1".to_owned());
    }
    if !range(dither.mouse_radius, 0.0, 2.0) {
        return Err("Dither mouse radius must be between 0 and 2".to_owned());
    }
    if !(2..=64).contains(&dither.color_num) {
        return Err("Dither color count must be between 2 and 64".to_owned());
    }
    if !(1..=16).contains(&dither.pixel_size) {
        return Err("Dither pixel size must be between 1 and 16".to_owned());
    }
    if let Some(color) = &dither.wave_color {
        validate_color(color)?;
    }
    validate_color(&dither.background_color)
}

fn validate_color(color: &str) -> Result<(), String> {
    let hex = color.len() == 7
        && color.starts_with('#')
        && color.is_ascii()
        && color[1..].chars().all(|digit| digit.is_ascii_hexdigit());
    if hex {
        Ok(())
    } else {
        Err("Dither colors must be hex values such as #ff8800".to_owned())
    }
}

fn validate_id(id: &str) -> Result<(), String> {
    if Uuid::parse_str(id).is_ok_and(|uuid| uuid.to_string() == id) {
        Ok(())
    } else {
        Err("Appearance asset IDs must be canonical UUIDs".to_owned())
    }
}

pub(crate) fn supports_transparency() -> bool {
    crate::commands::desktop_environment().as_deref() == Some("hyprland")
}

fn background_directory(app: &AppHandle) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    let directory = root.join("theme-backgrounds");
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Could not create theme directory: {error}"))?;
    if fs::symlink_metadata(&directory)
        .map_err(|error| error.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("Theme directory must not be a symbolic link".to_owned());
    }
    Ok(directory)
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    if !path.is_absolute() {
        return Err("Theme paths must be absolute".to_owned());
    }
    let file = File::open(path).map_err(|error| format!("Could not open theme file: {error}"))?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("Select a regular theme file".to_owned());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > limit {
        return Err(format!(
            "Theme file exceeds the {} MiB limit",
            limit / 1024 / 1024
        ));
    }
    Ok(bytes)
}

fn normalize_background(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err("Background images cannot exceed 16 MiB".to_owned());
    }
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| error.to_string())?;
    if !matches!(
        reader.format(),
        Some(image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP)
    ) {
        return Err("Backgrounds must be PNG, JPEG or WebP images".to_owned());
    }
    let (width, height) = reader
        .into_dimensions()
        .map_err(|error| format!("Could not inspect background: {error}"))?;
    if width == 0
        || height == 0
        || width > 8192
        || height > 8192
        || u64::from(width) * u64::from(height) > 16_777_216
    {
        return Err("Background dimensions exceed the 16 megapixel limit".to_owned());
    }
    let image = image::load_from_memory(bytes)
        .map_err(|error| format!("Could not decode background: {error}"))?;
    let image = if width > 2560 || height > 2560 {
        image.resize(2560, 2560, image::imageops::FilterType::Triangle)
    } else {
        image
    };
    let mut output = Cursor::new(Vec::new());
    image
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    let bytes = output.into_inner();
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err("Normalized background exceeds 16 MiB".to_owned());
    }
    Ok(bytes)
}

fn store_background(app: &AppHandle, bytes: &[u8]) -> Result<String, String> {
    let id = Uuid::new_v4().to_string();
    let path = background_directory(app)?.join(format!("{id}.png"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("Could not store background: {error}"))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| error.to_string())?;
    Ok(id)
}

fn background_path(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    validate_id(id)?;
    let path = background_directory(app)?.join(format!("{id}.png"));
    let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_IMAGE_BYTES
    {
        return Err("Background assets must be regular image files of at most 16 MiB".to_owned());
    }
    Ok(path)
}

fn load_background(app: &AppHandle, id: &str) -> Result<Vec<u8>, String> {
    read_bounded(&background_path(app, id)?, MAX_IMAGE_BYTES)
}

pub(crate) fn validate_background(app: &AppHandle, appearance: &Appearance) -> Result<(), String> {
    if let Some(id) = &appearance.background {
        background_path(app, id)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn import_theme_background(app: AppHandle, source: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = normalize_background(&read_bounded(Path::new(&source), MAX_IMAGE_BYTES)?)?;
        store_background(&app, &bytes)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn get_theme_background(
    app: AppHandle,
    id: String,
) -> Result<tauri::ipc::Response, String> {
    tauri::async_runtime::spawn_blocking(move || {
        load_background(&app, &id).map(tauri::ipc::Response::new)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn cleanup_theme_backgrounds(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<DatabaseState>();
        let current = state.database()?.settings()?.appearance.background;
        let directory = background_directory(&app)?;
        for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            let Some(id) = path.file_stem().and_then(|id| id.to_str()) else {
                continue;
            };
            if path.extension().is_some_and(|extension| extension == "png")
                && validate_id(id).is_ok()
                && Some(id) != current.as_deref()
            {
                fs::remove_file(path).map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_appearance_fields_are_tolerated() {
        let appearance: Appearance = serde_json::from_str(
            r#"{
                "customThemes": [],
                "customThemeId": null,
                "background": null,
                "backgroundBlur": 12,
                "backgroundOpacity": 60,
                "animatedBackground": "dither",
                "animatedOpacity": 65,
                "surfaceOpacity": 90,
                "surfaceBlur": 8,
                "dialogOpacity": 90,
                "dialogBlur": 8,
                "transparent": false
            }"#,
        )
        .unwrap();
        assert_eq!(appearance.animated_background, AnimatedBackground::Dither);
        assert_eq!(appearance.background_blur, 12);
        assert_eq!(appearance.surface_opacity, 90);
        assert_eq!(appearance.surface_blur, 8);
        assert_eq!(appearance.dialog_opacity, 90);
        assert_eq!(appearance.dialog_blur, 8);
        assert_eq!(appearance.dither, DitherSettings::default());
        let missing: Appearance = serde_json::from_str("{}").unwrap();
        assert_eq!(missing, Appearance::default());
    }

    #[test]
    fn unknown_animated_background_falls_back_to_none() {
        let unknown: Appearance =
            serde_json::from_str(r#"{"animatedBackground": "retro-glow"}"#).unwrap();
        assert_eq!(unknown.animated_background, AnimatedBackground::None);
        for background in [
            AnimatedBackground::None,
            AnimatedBackground::Particles,
            AnimatedBackground::Dither,
        ] {
            let encoded = serde_json::to_string(&background).unwrap();
            assert_eq!(
                serde_json::from_str::<AnimatedBackground>(&encoded).unwrap(),
                background
            );
        }
    }

    #[test]
    fn bounds_image_dimensions_and_file_reads() {
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(8193, 1)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        assert!(normalize_background(png.get_ref()).is_err());
        let path = std::env::temp_dir().join(format!("legio-theme-limit-{}", Uuid::new_v4()));
        fs::write(&path, b"123456789").unwrap();
        assert!(read_bounded(&path, 8).is_err());
        assert_eq!(read_bounded(&path, 9).unwrap(), b"123456789");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_out_of_bounds_background_effects() {
        let unsafe_id = Appearance {
            background: Some("../outside".to_owned()),
            ..Appearance::default()
        };
        assert!(validate(&unsafe_id).is_err());
        let blur = Appearance {
            background_blur: 41,
            ..Appearance::default()
        };
        assert!(validate(&blur).is_err());
        let opacity = Appearance {
            background_opacity: 101,
            ..Appearance::default()
        };
        assert!(validate(&opacity).is_err());
        let animated = Appearance {
            animated_opacity: 101,
            ..Appearance::default()
        };
        assert!(validate(&animated).is_err());
        let surfaces = Appearance {
            surface_opacity: 101,
            ..Appearance::default()
        };
        assert!(validate(&surfaces).is_err());
        let dialogs = Appearance {
            dialog_opacity: 101,
            ..Appearance::default()
        };
        assert!(validate(&dialogs).is_err());
        let surface_blur = Appearance {
            surface_blur: 41,
            ..Appearance::default()
        };
        assert!(validate(&surface_blur).is_err());
        let dialog_blur = Appearance {
            dialog_blur: 41,
            ..Appearance::default()
        };
        assert!(validate(&dialog_blur).is_err());
        let wave_speed = Appearance {
            dither: DitherSettings {
                wave_speed: 1.5,
                ..DitherSettings::default()
            },
            ..Appearance::default()
        };
        assert!(validate(&wave_speed).is_err());
        let color_num = Appearance {
            dither: DitherSettings {
                color_num: 1,
                ..DitherSettings::default()
            },
            ..Appearance::default()
        };
        assert!(validate(&color_num).is_err());
        let pixel_size = Appearance {
            dither: DitherSettings {
                pixel_size: 0,
                ..DitherSettings::default()
            },
            ..Appearance::default()
        };
        assert!(validate(&pixel_size).is_err());
        let wave_color = Appearance {
            dither: DitherSettings {
                wave_color: Some("red".to_owned()),
                ..DitherSettings::default()
            },
            ..Appearance::default()
        };
        assert!(validate(&wave_color).is_err());
        let background_color = Appearance {
            dither: DitherSettings {
                background_color: "#ff88".to_owned(),
                ..DitherSettings::default()
            },
            ..Appearance::default()
        };
        assert!(validate(&background_color).is_err());
        let wave_color = Appearance {
            dither: DitherSettings {
                wave_color: Some("#ff8800".to_owned()),
                ..DitherSettings::default()
            },
            ..Appearance::default()
        };
        assert!(validate(&wave_color).is_ok());
    }
}
