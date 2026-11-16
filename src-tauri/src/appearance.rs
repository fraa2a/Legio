use std::{
    fs::{self, File, OpenOptions},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::database::{DatabaseState, Theme};

const MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_PROFILE_BYTES: u64 = 64 * 1024;
const PROFILE_VERSION: u8 = 1;
const SHARE_CODE_PREFIX: &str = "LT";
const SHARE_CODE_VERSION: u8 = 1;
const MAX_SHARE_CODE_CHARS: usize = 4096;
const MAX_SHARE_JSON_BYTES: usize = 3 * MAX_SHARE_CODE_CHARS / 4;

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnimatedBackground {
    #[default]
    None,
    Particles,
    Dither,
    Scanlines,
    Aurora,
    Ambient,
    Starfield,
    Seasonal,
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
            "scanlines" => Self::Scanlines,
            "aurora" => Self::Aurora,
            "ambient" => Self::Ambient,
            "starfield" => Self::Starfield,
            "seasonal" => Self::Seasonal,
            _ => Self::None,
        })
    }
}

fn default_animated_opacity() -> u8 {
    65
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Density {
    Compact,
    Wide,
    // Unknown values from newer versions fall back to the default.
    #[default]
    #[serde(other)]
    Normal,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Corners {
    Sharp,
    Round,
    #[default]
    #[serde(other)]
    Soft,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Season {
    Rain,
    Leaves,
    #[default]
    #[serde(other)]
    Snow,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StarDensity {
    Low,
    High,
    #[default]
    #[serde(other)]
    Medium,
}

const MIN_TEXT_SCALE: u8 = 90;
const MAX_TEXT_SCALE: u8 = 120;

fn default_text_scale() -> u8 {
    100
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
    pub accent: Option<String>,
    pub background: Option<String>,
    pub background_blur: u8,
    pub background_opacity: u8,
    pub animated_background: AnimatedBackground,
    pub animated_opacity: u8,
    pub dither: DitherSettings,
    pub season: Season,
    pub star_density: StarDensity,
    // Noise overlay over any background, 0 is off.
    pub grain: u8,
    pub surface_opacity: u8,
    pub surface_blur: u8,
    pub dialog_opacity: u8,
    pub dialog_blur: u8,
    pub transparent: bool,
    pub density: Density,
    pub corners: Corners,
    pub text_scale: u8,
    pub game_accent: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            accent: None,
            background: None,
            background_blur: 0,
            background_opacity: 60,
            animated_background: AnimatedBackground::None,
            animated_opacity: default_animated_opacity(),
            dither: DitherSettings::default(),
            season: Season::Snow,
            star_density: StarDensity::Medium,
            grain: 0,
            surface_opacity: 45,
            surface_blur: 24,
            dialog_opacity: 45,
            dialog_blur: 35,
            transparent: false,
            density: Density::Normal,
            corners: Corners::Soft,
            text_scale: default_text_scale(),
            game_accent: false,
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
        || appearance.grain > 100
    {
        return Err(
            "Blur values must be 0-40 and opacity and grain values must be 0-100".to_owned(),
        );
    }
    if !(MIN_TEXT_SCALE..=MAX_TEXT_SCALE).contains(&appearance.text_scale) {
        return Err("Text scale must be between 90 and 120 percent".to_owned());
    }
    if let Some(id) = &appearance.background {
        validate_id(id)?;
    }
    if let Some(color) = &appearance.accent {
        validate_color(color)?;
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
        Err("Colors must be hex values such as #ff8800".to_owned())
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
    tauri::async_runtime::spawn_blocking(move || import_background_file(&app, Path::new(&source)))
        .await
        .map_err(|error| error.to_string())?
}

/// Normalizes an image file into a stored theme background and returns its id.
pub(crate) fn import_background_file(app: &AppHandle, source: &Path) -> Result<String, String> {
    let bytes = normalize_background(&read_bounded(source, MAX_IMAGE_BYTES)?)?;
    store_background(app, &bytes)
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppearanceProfile {
    version: u8,
    theme: Theme,
    appearance: Appearance,
}

impl AppearanceProfile {
    // Background images live in the local app data, so profiles never carry them.
    fn checked(mut self) -> Result<Self, String> {
        if self.version != PROFILE_VERSION {
            return Err("Unsupported appearance file version".to_owned());
        }
        self.appearance.background = None;
        validate(&self.appearance)?;
        Ok(self)
    }
}

fn share_code_header() -> String {
    format!("{SHARE_CODE_PREFIX}{SHARE_CODE_VERSION}.")
}

impl AppearanceProfile {
    fn share_code(self) -> Result<String, String> {
        let json = serde_json::to_vec(&self.checked()?).map_err(|error| error.to_string())?;
        if json.len() > MAX_SHARE_JSON_BYTES {
            return Err("The theme is too large to be shared as a code".to_owned());
        }
        Ok(share_code_header() + &URL_SAFE_NO_PAD.encode(json))
    }

    // Codes arrive from the clipboard or a link, so they are validated like an imported file.
    pub(crate) fn from_share_code(code: &str) -> Result<Self, String> {
        let code = code.trim();
        if code.len() > MAX_SHARE_CODE_CHARS {
            return Err("The theme code is too long".to_owned());
        }
        let header = share_code_header();
        let Some(encoded) = code.strip_prefix(&header) else {
            let version = code
                .split_once('.')
                .and_then(|(head, _)| head.strip_prefix(SHARE_CODE_PREFIX));
            return Err(
                if version.is_some_and(|digits| {
                    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
                }) {
                    "Unsupported theme code version".to_owned()
                } else {
                    "The text is not a Legio theme code".to_owned()
                },
            );
        };
        let json = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| "The theme code is corrupted".to_owned())?;
        if json.len() > MAX_SHARE_JSON_BYTES {
            return Err("The theme code is too long".to_owned());
        }
        serde_json::from_slice::<Self>(&json)
            .map_err(|_| "The theme code is corrupted".to_owned())?
            .checked()
    }
}

#[tauri::command]
pub fn create_theme_code(profile: AppearanceProfile) -> Result<String, String> {
    profile.share_code()
}

#[tauri::command]
pub fn read_theme_code(code: String) -> Result<AppearanceProfile, String> {
    AppearanceProfile::from_share_code(&code)
}

#[tauri::command]
pub async fn export_appearance(path: String, profile: AppearanceProfile) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = Path::new(&path);
        if !path.is_absolute() {
            return Err("Appearance paths must be absolute".to_owned());
        }
        let json =
            serde_json::to_vec_pretty(&profile.checked()?).map_err(|error| error.to_string())?;
        fs::write(path, json).map_err(|error| format!("Could not write appearance file: {error}"))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn import_appearance(path: String) -> Result<AppearanceProfile, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = read_bounded(Path::new(&path), MAX_PROFILE_BYTES)?;
        serde_json::from_slice::<AppearanceProfile>(&bytes)
            .map_err(|_| "The file is not a valid appearance export".to_owned())?
            .checked()
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
    fn new_installs_default_to_the_glass_panel_style() {
        // Keep in sync with panelStyles.glass in the frontend appearance service.
        let defaults = Appearance::default();
        assert_eq!(
            (
                defaults.surface_opacity,
                defaults.surface_blur,
                defaults.dialog_opacity,
                defaults.dialog_blur
            ),
            (45, 24, 45, 35)
        );
    }

    #[test]
    fn interface_options_default_and_tolerate_unknown_values() {
        let missing: Appearance = serde_json::from_str("{}").unwrap();
        assert_eq!(missing.density, Density::Normal);
        assert_eq!(missing.corners, Corners::Soft);
        assert_eq!(missing.text_scale, 100);
        let chosen: Appearance =
            serde_json::from_str(r#"{"density": "compact", "corners": "round", "textScale": 110}"#)
                .unwrap();
        assert_eq!(chosen.density, Density::Compact);
        assert_eq!(chosen.corners, Corners::Round);
        assert_eq!(chosen.text_scale, 110);
        let unknown: Appearance =
            serde_json::from_str(r#"{"density": "huge", "corners": "blob"}"#).unwrap();
        assert_eq!(unknown.density, Density::Normal);
        assert_eq!(unknown.corners, Corners::Soft);
    }

    #[test]
    fn rejects_out_of_range_text_scale() {
        for text_scale in [89, 121] {
            let scale = Appearance {
                text_scale,
                ..Appearance::default()
            };
            assert!(validate(&scale).is_err());
        }
        for text_scale in [90, 120] {
            let scale = Appearance {
                text_scale,
                ..Appearance::default()
            };
            assert!(validate(&scale).is_ok());
        }
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
            AnimatedBackground::Scanlines,
            AnimatedBackground::Aurora,
            AnimatedBackground::Ambient,
            AnimatedBackground::Starfield,
            AnimatedBackground::Seasonal,
        ] {
            let encoded = serde_json::to_string(&background).unwrap();
            assert_eq!(
                serde_json::from_str::<AnimatedBackground>(&encoded).unwrap(),
                background
            );
        }
    }

    #[test]
    fn background_options_round_trip_and_default_for_older_appearances() {
        let legacy: Appearance = serde_json::from_str(
            r#"{"animatedBackground": "aurora", "animatedOpacity": 40, "dither": {}}"#,
        )
        .unwrap();
        assert_eq!(legacy.animated_background, AnimatedBackground::Aurora);
        assert_eq!(legacy.season, Season::Snow);
        assert_eq!(legacy.star_density, StarDensity::Medium);
        assert_eq!(legacy.grain, 0);
        assert!(validate(&legacy).is_ok());
        let chosen = Appearance {
            animated_background: AnimatedBackground::Seasonal,
            season: Season::Leaves,
            star_density: StarDensity::High,
            grain: 30,
            ..Appearance::default()
        };
        let json = serde_json::to_string(&chosen).unwrap();
        assert!(json.contains(r#""animatedBackground":"seasonal""#));
        assert!(json.contains(r#""season":"leaves""#));
        assert!(json.contains(r#""starDensity":"high""#));
        assert_eq!(serde_json::from_str::<Appearance>(&json).unwrap(), chosen);
        let unknown: Appearance =
            serde_json::from_str(r#"{"season": "monsoon", "starDensity": "galaxy"}"#).unwrap();
        assert_eq!(unknown.season, Season::Snow);
        assert_eq!(unknown.star_density, StarDensity::Medium);
        let loud = Appearance {
            grain: 101,
            ..Appearance::default()
        };
        assert!(validate(&loud).is_err());
    }

    #[test]
    fn share_code_carries_the_background_options() {
        let mut profile = shared_profile();
        profile.appearance.animated_background = AnimatedBackground::Starfield;
        profile.appearance.star_density = StarDensity::Low;
        profile.appearance.season = Season::Rain;
        profile.appearance.grain = 25;
        let decoded = AppearanceProfile::from_share_code(&profile.share_code().unwrap()).unwrap();
        assert_eq!(
            decoded.appearance.animated_background,
            AnimatedBackground::Starfield
        );
        assert_eq!(decoded.appearance.star_density, StarDensity::Low);
        assert_eq!(decoded.appearance.season, Season::Rain);
        assert_eq!(decoded.appearance.grain, 25);
        assert_eq!(decoded.appearance.background, None);
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

    #[test]
    fn accent_color_must_be_a_hex_value() {
        for accent in ["#c8893a", "#FFFFFF"] {
            let valid = Appearance {
                accent: Some(accent.to_owned()),
                ..Appearance::default()
            };
            assert!(validate(&valid).is_ok());
        }
        for accent in [
            "c8893a",
            "#c8893",
            "#c8893g",
            "red",
            "#c8893a; x",
            "#é8893a",
        ] {
            let invalid = Appearance {
                accent: Some(accent.to_owned()),
                ..Appearance::default()
            };
            assert!(validate(&invalid).is_err(), "{accent}");
        }
        let missing: Appearance = serde_json::from_str("{}").unwrap();
        assert_eq!(missing.accent, None);
    }

    #[test]
    fn profile_roundtrip_drops_the_background_and_rejects_bad_files() {
        let profile = AppearanceProfile {
            version: PROFILE_VERSION,
            theme: Theme::Dark,
            appearance: Appearance {
                background: Some("a".repeat(32)),
                dialog_opacity: 50,
                ..Appearance::default()
            },
        };
        let checked = profile.clone().checked().unwrap();
        assert_eq!(checked.appearance.background, None);
        assert_eq!(checked.appearance.dialog_opacity, 50);
        let json = serde_json::to_string(&checked).unwrap();
        assert_eq!(
            serde_json::from_str::<AppearanceProfile>(&json).unwrap(),
            checked
        );
        let mut future = profile.clone();
        future.version = PROFILE_VERSION + 1;
        assert!(future.checked().is_err());
        let mut loud = profile;
        loud.appearance.dialog_blur = 41;
        assert!(loud.checked().is_err());
    }

    fn shared_profile() -> AppearanceProfile {
        AppearanceProfile {
            version: PROFILE_VERSION,
            theme: Theme::Violet,
            appearance: Appearance {
                background: Some("a".repeat(32)),
                accent: Some("#a67cf0".to_owned()),
                game_accent: true,
                ..Appearance::default()
            },
        }
    }

    #[test]
    fn share_code_round_trips_without_the_background() {
        let code = shared_profile().share_code().unwrap();
        assert!(code.starts_with("LT1."));
        assert!(code.len() < 1024);
        assert!(
            code[4..]
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        );
        let decoded = AppearanceProfile::from_share_code(&format!("  {code}\n")).unwrap();
        assert_eq!(decoded.theme, Theme::Violet);
        assert_eq!(decoded.appearance.background, None);
        assert!(decoded.appearance.game_accent);
        assert_eq!(decoded.appearance.accent.as_deref(), Some("#a67cf0"));
    }

    #[test]
    fn share_code_rejects_oversized_garbled_and_unknown_codes() {
        let code = shared_profile().share_code().unwrap();
        let oversized = format!("LT1.{}", "A".repeat(MAX_SHARE_CODE_CHARS));
        assert!(AppearanceProfile::from_share_code(&oversized).is_err());
        let long_json = URL_SAFE_NO_PAD.encode(vec![b' '; MAX_SHARE_JSON_BYTES + 1]);
        assert!(AppearanceProfile::from_share_code(&format!("LT1.{long_json}")).is_err());
        for garbled in [
            "",
            "LT1.",
            "LT1.@@@",
            "LT1.e30",
            "hello",
            &code[..code.len() - 7],
        ] {
            assert!(
                AppearanceProfile::from_share_code(garbled).is_err(),
                "{garbled}"
            );
        }
        let future = code.replacen("LT1.", "LT2.", 1);
        assert_eq!(
            AppearanceProfile::from_share_code(&future).unwrap_err(),
            "Unsupported theme code version"
        );
        let mut loud = shared_profile();
        loud.appearance.dialog_blur = 41;
        let json = serde_json::to_vec(&loud).unwrap();
        let invalid = format!("LT1.{}", URL_SAFE_NO_PAD.encode(json));
        assert!(AppearanceProfile::from_share_code(&invalid).is_err());
        let mut old = shared_profile();
        old.version = PROFILE_VERSION + 1;
        assert!(old.share_code().is_err());
    }
}
