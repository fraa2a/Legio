//! Opens Thunderstore package pages in the default browser.

use super::thunderstore_http::is_allowed_url;

#[tauri::command]
pub async fn open_mod_package_page(url: String) -> Result<(), String> {
    let url = parse_page_url(&url)?;
    tauri::async_runtime::spawn_blocking(move || open_in_browser(&url))
        .await
        .map_err(|error| format!("Package page task failed: {error}"))?
}

fn parse_page_url(url: &str) -> Result<reqwest::Url, String> {
    reqwest::Url::parse(url)
        .ok()
        .filter(is_allowed_url)
        .ok_or_else(|| "The package page is not a Thunderstore address".to_owned())
}

#[cfg(windows)]
fn open_in_browser(url: &reqwest::Url) -> Result<(), String> {
    // explorer.exe hands URLs to the default browser and, as for folders,
    // reports a failure status even when it succeeds.
    crate::background_process::command("explorer.exe")
        .arg(url.as_str())
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not open the package page: {error}"))
}

#[cfg(target_os = "linux")]
fn open_in_browser(url: &reqwest::Url) -> Result<(), String> {
    let mut command = std::process::Command::new("xdg-open");
    command.arg(url.as_str());
    crate::launch_environment::apply(&mut command);
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not open the package page: {error}"))
}

#[cfg(test)]
mod tests {
    use super::parse_page_url;

    #[test]
    fn accepts_only_thunderstore_pages() {
        assert!(parse_page_url("https://thunderstore.io/c/lethal-company/p/Owner/Name/").is_ok());
        for url in ["https://example.com/", "file:///C:/Windows", "not a url"] {
            assert!(parse_page_url(url).is_err(), "{url}");
        }
    }
}
