use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::database::{DatabaseState, Settings, Theme};

const MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_THEME_BYTES: u64 = 24 * 1024 * 1024;
const MAX_CUSTOM_THEMES: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Palette {
    pub background: String,
    pub surface: String,
    pub raised: String,
    pub text: String,
    pub muted: String,
    pub accent: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ColorScheme {
    Dark,
    Light,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CustomTheme {
    pub id: String,
    pub name: String,
    pub scheme: ColorScheme,
    pub palette: Palette,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnimatedBackground {
    #[default]
    None,
    Particles,
    #[serde(alias = "aurora")]
    Dither,
}

fn default_animated_opacity() -> u8 {
    65
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Appearance {
    pub custom_themes: Vec<CustomTheme>,
    pub custom_theme_id: Option<String>,
    pub background: Option<String>,
    pub background_blur: u8,
    pub background_opacity: u8,
    pub animated_background: AnimatedBackground,
    pub animated_opacity: u8,
    pub surface_opacity: u8,
    pub surface_blur: u8,
    pub dialog_opacity: u8,
    pub dialog_blur: u8,
    pub transparent: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            custom_themes: Vec::new(),
            custom_theme_id: None,
            background: None,
            background_blur: 0,
            background_opacity: 60,
            animated_background: AnimatedBackground::None,
            animated_opacity: default_animated_opacity(),
            surface_opacity: 90,
            surface_blur: 0,
            dialog_opacity: default_dialog_opacity(),
            dialog_blur: 0,
            transparent: false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ThemeFile {
    schema_version: u8,
    name: String,
    scheme: ColorScheme,
    palette: Palette,
    background_image: Option<String>,
    background_blur: u8,
    background_opacity: u8,
    #[serde(default)]
    animated_background: AnimatedBackground,
    #[serde(default = "default_animated_opacity")]
    animated_opacity: u8,
    surface_opacity: u8,
    #[serde(default)]
    surface_blur: u8,
    #[serde(default = "default_dialog_opacity")]
    dialog_opacity: u8,
    #[serde(default)]
    dialog_blur: u8,
    transparent: bool,
}

fn default_dialog_opacity() -> u8 {
    90
}

pub(crate) fn validate(appearance: &Appearance, theme: &Theme) -> Result<(), String> {
    if appearance.custom_themes.len() > MAX_CUSTOM_THEMES {
        return Err("At most 20 custom themes can be saved".to_owned());
    }
    let mut ids = HashSet::new();
    for custom in &appearance.custom_themes {
        validate_id(&custom.id)?;
        if !ids.insert(&custom.id) {
            return Err("Custom theme IDs must be unique".to_owned());
        }
        validate_name(&custom.name)?;
        validate_palette(&custom.palette)?;
        validate_scheme(&custom.palette, &custom.scheme)?;
    }
    if let Some(id) = &appearance.custom_theme_id {
        validate_id(id)?;
        if !ids.contains(id) {
            return Err("The selected custom theme does not exist".to_owned());
        }
    } else if *theme == Theme::Custom {
        return Err("Select a custom palette before enabling a custom theme".to_owned());
    }
    if let Some(id) = &appearance.background {
        validate_id(id)?;
    }
    if appearance.background_blur > 40
        || appearance.background_opacity > 100
        || appearance.animated_opacity > 100
        || appearance.surface_opacity > 100
        || appearance.dialog_opacity > 100
        || appearance.surface_blur > 40
        || appearance.dialog_blur > 40
    {
        return Err("Blur values must be 0-40 and opacity values must be 0-100".to_owned());
    }
    Ok(())
}

fn validate_id(id: &str) -> Result<(), String> {
    if Uuid::parse_str(id).is_ok_and(|uuid| uuid.to_string() == id) {
        Ok(())
    } else {
        Err("Appearance asset IDs must be canonical UUIDs".to_owned())
    }
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
        return Err(
            "Theme names must contain 1-64 characters without control characters".to_owned(),
        );
    }
    Ok(())
}

fn rgb(color: &str) -> Result<[u8; 3], String> {
    let bytes = color.as_bytes();
    if bytes.len() != 7 || bytes[0] != b'#' || !bytes[1..].iter().all(u8::is_ascii_hexdigit) {
        return Err("Palette colors must use #RRGGBB notation".to_owned());
    }
    let channel = |offset| {
        u8::from_str_radix(&color[offset..offset + 2], 16)
            .map_err(|_| "Invalid palette color".to_owned())
    };
    Ok([channel(1)?, channel(3)?, channel(5)?])
}

fn luminance(color: [u8; 3]) -> f64 {
    let linear = color.map(|value| {
        let value = f64::from(value) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    });
    linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722
}

fn validate_palette(palette: &Palette) -> Result<(), String> {
    for color in [
        &palette.background,
        &palette.surface,
        &palette.raised,
        &palette.text,
        &palette.muted,
        &palette.accent,
    ] {
        rgb(color)?;
    }
    let text = luminance(rgb(&palette.text)?);
    for background in [&palette.background, &palette.surface, &palette.raised] {
        let background = luminance(rgb(background)?);
        if (text.max(background) + 0.05) / (text.min(background) + 0.05) < 4.5 {
            return Err(
                "Theme text needs a contrast ratio of at least 4.5 against every panel".to_owned(),
            );
        }
    }
    Ok(())
}

fn validate_scheme(palette: &Palette, scheme: &ColorScheme) -> Result<(), String> {
    let text = luminance(rgb(&palette.text)?);
    for background in [&palette.background, &palette.surface, &palette.raised] {
        let background = luminance(rgb(background)?);
        if matches!(scheme, ColorScheme::Dark) != (text > background) {
            return Err(
                "The text and panel colors must match the selected light or dark scheme".to_owned(),
            );
        }
    }
    Ok(())
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
pub async fn import_theme_file(app: AppHandle, source: String) -> Result<Settings, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = read_bounded(Path::new(&source), MAX_THEME_BYTES)?;
        let document: ThemeFile = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Invalid theme JSON: {error}"))?;
        if document.schema_version != 1 {
            return Err("Unsupported theme schema version".to_owned());
        }
        let state = app.state::<DatabaseState>();
        let mut settings = state.database()?.settings()?;
        let id = Uuid::new_v4().to_string();
        settings.theme = Theme::Custom;
        settings.appearance.custom_theme_id = Some(id.clone());
        settings.appearance.custom_themes.push(CustomTheme {
            id,
            name: document.name,
            scheme: document.scheme,
            palette: document.palette,
        });
        settings.appearance.background = None;
        settings.appearance.background_blur = document.background_blur;
        settings.appearance.background_opacity = document.background_opacity;
        settings.appearance.animated_background = document.animated_background;
        settings.appearance.animated_opacity = document.animated_opacity;
        settings.appearance.surface_opacity = document.surface_opacity;
        settings.appearance.surface_blur = document.surface_blur;
        settings.appearance.dialog_opacity = document.dialog_opacity;
        settings.appearance.dialog_blur = document.dialog_blur;
        settings.appearance.transparent = document.transparent;
        validate(&settings.appearance, &settings.theme)?;
        if let Some(encoded) = document.background_image {
            let bytes = STANDARD
                .decode(encoded)
                .map_err(|_| "Invalid background encoding".to_owned())?;
            let normalized = normalize_background(&bytes)?;
            settings.appearance.background = Some(store_background(&app, &normalized)?);
        }
        state.database()?.save_settings(settings)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn export_theme_file(
    app: AppHandle,
    destination: String,
    prefers_dark: bool,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<DatabaseState>();
        let settings = state.database()?.settings()?;
        let palette = active_palette(&settings, prefers_dark)?;
        let background_image = settings
            .appearance
            .background
            .as_deref()
            .map(|id| load_background(&app, id).map(|bytes| STANDARD.encode(bytes)))
            .transpose()?;
        let document = ThemeFile {
            schema_version: 1,
            name: palette.name,
            scheme: palette.scheme,
            palette: palette.palette,
            background_image,
            background_blur: settings.appearance.background_blur,
            background_opacity: settings.appearance.background_opacity,
            animated_background: settings.appearance.animated_background,
            animated_opacity: settings.appearance.animated_opacity,
            surface_opacity: settings.appearance.surface_opacity,
            surface_blur: settings.appearance.surface_blur,
            dialog_opacity: settings.appearance.dialog_opacity,
            dialog_blur: settings.appearance.dialog_blur,
            transparent: settings.appearance.transparent,
        };
        let bytes = serde_json::to_vec_pretty(&document).map_err(|error| error.to_string())?;
        let path = Path::new(&destination);
        if !path.is_absolute()
            || path
                .extension()
                .is_none_or(|extension| !extension.eq_ignore_ascii_case("json"))
        {
            return Err("Choose an absolute .json export path".to_owned());
        }
        // Existing files are preserved; choose a new filename to export again.
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| format!("Could not create theme export: {error}"))?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

fn active_palette(settings: &Settings, prefers_dark: bool) -> Result<CustomTheme, String> {
    if settings.theme == Theme::Custom {
        return settings
            .appearance
            .custom_themes
            .iter()
            .find(|theme| Some(&theme.id) == settings.appearance.custom_theme_id.as_ref())
            .cloned()
            .ok_or_else(|| "The selected custom theme does not exist".to_owned());
    }
    let presets: Vec<CustomTheme> =
        serde_json::from_str(include_str!("../../src/lib/services/theme-presets.json"))
            .map_err(|error| format!("Could not load built-in themes: {error}"))?;
    let id = match settings.theme {
        Theme::System if prefers_dark => "dark",
        Theme::System | Theme::Light => "light",
        Theme::Dark => "dark",
        Theme::Eggplant => "eggplant",
        Theme::Ocean => "ocean",
        Theme::Forest => "forest",
        Theme::Amber => "amber",
        Theme::Custom => return Err("Select a custom theme".to_owned()),
    };
    presets
        .into_iter()
        .find(|preset| preset.id == id)
        .ok_or_else(|| "Built-in palette is missing".to_owned())
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
    fn legacy_aurora_background_deserializes_as_dither() {
        let background: AnimatedBackground = serde_json::from_str("\"aurora\"").unwrap();
        assert_eq!(background, AnimatedBackground::Dither);
        assert_eq!(serde_json::to_string(&background).unwrap(), "\"dither\"");
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
    fn presets_have_readable_palettes() {
        let presets: Vec<CustomTheme> =
            serde_json::from_str(include_str!("../../src/lib/services/theme-presets.json"))
                .unwrap();
        for preset in presets {
            validate_palette(&preset.palette).unwrap();
            validate_scheme(&preset.palette, &preset.scheme).unwrap();
        }
    }

    #[test]
    fn imported_palettes_cannot_inject_css_or_hide_text() {
        let mut palette = active_palette(&Settings::default(), true).unwrap().palette;
        palette.accent = "url(https://example.com)".to_owned();
        assert!(validate_palette(&palette).is_err());
        palette.accent = "#ffffff".to_owned();
        palette.text = palette.surface.clone();
        assert!(validate_palette(&palette).is_err());
        palette.text = "#é0000".to_owned();
        assert!(validate_palette(&palette).is_err());
    }

    #[test]
    fn rejects_missing_custom_palettes_and_out_of_bounds_effects() {
        let mut appearance = Appearance::default();
        assert!(validate(&appearance, &Theme::Custom).is_err());
        appearance.background = Some("../outside".to_owned());
        assert!(validate(&appearance, &Theme::Dark).is_err());
        appearance.background = None;
        appearance.background_blur = 41;
        assert!(validate(&appearance, &Theme::Dark).is_err());
        appearance.background_blur = 0;
        appearance.surface_blur = 41;
        assert!(validate(&appearance, &Theme::Dark).is_err());
        appearance.surface_blur = 0;
        appearance.dialog_opacity = 101;
        assert!(validate(&appearance, &Theme::Dark).is_err());
        appearance.dialog_opacity = 90;
        appearance.dialog_blur = 41;
        assert!(validate(&appearance, &Theme::Dark).is_err());
    }

    #[test]
    fn portable_theme_round_trip_preserves_palette_and_background() {
        let preset = active_palette(
            &Settings {
                theme: Theme::Eggplant,
                ..Settings::default()
            },
            false,
        )
        .unwrap();
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(4, 4)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let normalized = normalize_background(png.get_ref()).unwrap();
        assert_eq!(image::load_from_memory(&normalized).unwrap().width(), 4);
        let document = ThemeFile {
            schema_version: 1,
            name: preset.name,
            scheme: preset.scheme,
            palette: preset.palette,
            background_image: Some(STANDARD.encode(&normalized)),
            background_blur: 12,
            background_opacity: 50,
            animated_background: AnimatedBackground::Particles,
            animated_opacity: 70,
            surface_opacity: 80,
            surface_blur: 12,
            dialog_opacity: 75,
            dialog_blur: 8,
            transparent: true,
        };
        let decoded: ThemeFile =
            serde_json::from_slice(&serde_json::to_vec(&document).unwrap()).unwrap();
        assert_eq!(decoded.name, "Palette Viola Melanzana");
        assert_eq!(
            STANDARD.decode(decoded.background_image.unwrap()).unwrap(),
            normalized
        );
        assert_eq!(decoded.background_blur, 12);
        assert_eq!(decoded.surface_blur, 12);
        assert_eq!(decoded.dialog_opacity, 75);
        assert_eq!(decoded.dialog_blur, 8);
        let mut old_document = serde_json::to_value(&document).unwrap();
        old_document.as_object_mut().unwrap().remove("surfaceBlur");
        old_document
            .as_object_mut()
            .unwrap()
            .remove("dialogOpacity");
        old_document.as_object_mut().unwrap().remove("dialogBlur");
        let old_document: ThemeFile = serde_json::from_value(old_document).unwrap();
        assert_eq!(old_document.surface_blur, 0);
        assert_eq!(old_document.dialog_opacity, 90);
        assert_eq!(old_document.dialog_blur, 0);
        assert!(normalize_background(b"<svg onload='alert(1)'/>").is_err());
    }
}
