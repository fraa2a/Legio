use serde::Serialize;
use tauri::{AppHandle, Manager, ResourceId, Url, Webview};
use tauri_plugin_updater::UpdaterExt;

use crate::database::DatabaseState;

// Fixed release whose asset the pipeline refreshes on every release, stable ones included,
// so a preview user is never offered less than the newest stable version.
const PREVIEW_MANIFEST: &str =
    "https://github.com/fraa2a/Legio/releases/download/preview-channel/latest-preview.json";

/// Same shape as the updater plugin's own `check` result, so the plugin's JS `Update` wraps it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateMetadata {
    rid: ResourceId,
    current_version: String,
    version: String,
    body: Option<String>,
    raw_json: serde_json::Value,
}

#[derive(Serialize)]
pub(crate) struct UpdateCheck {
    update: Option<UpdateMetadata>,
    /// The manifest has no build for this platform.
    unavailable: bool,
}

pub(crate) async fn check(
    app: &AppHandle,
    webview: Webview,
    aur: bool,
) -> Result<UpdateCheck, String> {
    let preview = !aur && preview_updates_enabled(app).await?;
    let mut builder = webview.updater_builder();
    if preview {
        let endpoint = Url::parse(PREVIEW_MANIFEST).map_err(|error| error.to_string())?;
        builder = builder
            .endpoints(vec![endpoint])
            .map_err(|error| error.to_string())?;
    }
    let updater = builder.build().map_err(|error| error.to_string())?;
    match updater.check().await {
        Ok(update) => Ok(UpdateCheck {
            update: update.map(|update| UpdateMetadata {
                current_version: update.current_version.clone(),
                version: update.version.clone(),
                body: update.body.clone(),
                raw_json: update.raw_json.clone(),
                rid: webview.resources_table().add(update),
            }),
            unavailable: false,
        }),
        Err(
            tauri_plugin_updater::Error::TargetNotFound(_)
            | tauri_plugin_updater::Error::TargetsNotFound(_),
        ) => Ok(UpdateCheck {
            update: None,
            unavailable: true,
        }),
        Err(error) => Err(error.to_string()),
    }
}

async fn preview_updates_enabled(app: &AppHandle) -> Result<bool, String> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        Ok(app
            .state::<DatabaseState>()
            .database()?
            .settings()?
            .preview_updates)
    })
    .await
    .map_err(|error| format!("Settings load task failed: {error}"))?
}
