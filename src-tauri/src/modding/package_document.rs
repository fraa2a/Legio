//! Readme and changelog of a package version, fetched from the Thunderstore
//! experimental API and rendered to sanitized HTML.

use serde::Deserialize;
use tauri::State;

use super::{
    markdown_html::render_markdown,
    package_id::{PackageName, Version},
    thunderstore_http::{ThunderstoreHttp, describe},
};
use crate::network::NetworkError;

const API_BASE: &str = "https://thunderstore.io";
const MAX_DOCUMENT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PackageDocument {
    Readme,
    Changelog,
}

impl PackageDocument {
    fn path_segment(self) -> &'static str {
        match self {
            Self::Readme => "readme",
            Self::Changelog => "changelog",
        }
    }
}

#[derive(Deserialize)]
struct DocumentBody {
    markdown: Option<String>,
}

/// Sanitized HTML of a package document, or `None` when the version has none.
#[tauri::command]
pub async fn get_mod_package_document(
    http: State<'_, ThunderstoreHttp>,
    full_name: String,
    version: String,
    document: PackageDocument,
) -> Result<Option<String>, String> {
    let name: PackageName = full_name.parse()?;
    let version: Version = version.parse()?;
    fetch_document(&http, API_BASE, &name, version, document).await
}

async fn fetch_document(
    http: &ThunderstoreHttp,
    base: &str,
    name: &PackageName,
    version: Version,
    document: PackageDocument,
) -> Result<Option<String>, String> {
    let url = format!(
        "{base}/api/experimental/package/{}/{}/{version}/{}/",
        name.owner,
        name.name,
        document.path_segment()
    );
    let bytes = match http.try_get_bytes(&url, MAX_DOCUMENT_BYTES).await {
        Ok(bytes) => bytes,
        Err(NetworkError::Http { status: 404 }) => return Ok(None),
        Err(error) => return Err(describe(&error)),
    };
    tauri::async_runtime::spawn_blocking(move || render_document(&bytes))
        .await
        .map_err(|error| format!("Package document task failed: {error}"))?
}

fn render_document(bytes: &[u8]) -> Result<Option<String>, String> {
    let body: DocumentBody = serde_json::from_slice(bytes)
        .map_err(|error| format!("The Thunderstore package document is invalid: {error}"))?;
    Ok(body
        .markdown
        .filter(|markdown| !markdown.trim().is_empty())
        .map(|markdown| render_markdown(&markdown)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        diagnostics::Diagnostics,
        modding::test_support::{Reply, TestServer},
    };
    use serde_json::json;

    fn http() -> ThunderstoreHttp {
        ThunderstoreHttp::new(
            "0.1.0",
            Diagnostics::new(Err("test logs disabled".to_owned())),
        )
        .unwrap()
    }

    fn fetch(
        server: &TestServer,
        document: PackageDocument,
        version: &str,
    ) -> Result<Option<String>, String> {
        let base = server.url("");
        tauri::async_runtime::block_on(fetch_document(
            &http(),
            &base,
            &"Alice-Big_Mod".parse().unwrap(),
            version.parse().unwrap(),
            document,
        ))
    }

    #[test]
    fn renders_a_readme_from_the_experimental_api() {
        let server = TestServer::start();
        server.set(
            "/api/experimental/package/Alice/Big_Mod/1.2.0/readme/",
            Reply::ok(
                json!({"markdown": "# Hi\n\n<script>x</script> [a](javascript:1)"})
                    .to_string()
                    .into_bytes(),
            ),
        );
        let html = fetch(&server, PackageDocument::Readme, "1.2.0")
            .unwrap()
            .unwrap();
        assert!(html.contains("<h1>Hi</h1>"), "{html}");
        assert!(!html.contains("<script"), "{html}");
        assert!(!html.contains("<a "), "{html}");
        assert_eq!(
            server.requests(),
            ["/api/experimental/package/Alice/Big_Mod/1.2.0/readme/"]
        );
    }

    #[test]
    fn asks_for_the_changelog_path() {
        let server = TestServer::start();
        server.set(
            "/api/experimental/package/Alice/Big_Mod/1.2.0/changelog/",
            Reply::ok(json!({"markdown": "- **1.2.0**"}).to_string().into_bytes()),
        );
        let html = fetch(&server, PackageDocument::Changelog, "1.2.0")
            .unwrap()
            .unwrap();
        assert!(html.contains("<strong>1.2.0</strong>"), "{html}");
    }

    #[test]
    fn a_missing_document_is_none() {
        let server = TestServer::start();
        server.set(
            "/api/experimental/package/Alice/Big_Mod/1.2.0/changelog/",
            Reply::status(404),
        );
        assert_eq!(
            fetch(&server, PackageDocument::Changelog, "1.2.0"),
            Ok(None)
        );
        for body in [json!({"markdown": null}), json!({"markdown": "  \n"})] {
            server.set(
                "/api/experimental/package/Alice/Big_Mod/1.2.0/readme/",
                Reply::ok(body.to_string().into_bytes()),
            );
            assert_eq!(fetch(&server, PackageDocument::Readme, "1.2.0"), Ok(None));
        }
    }

    #[test]
    fn reports_other_failures() {
        let server = TestServer::start();
        let path = "/api/experimental/package/Alice/Big_Mod/1.2.0/readme/";
        server.set(path, Reply::status(500));
        let error = fetch(&server, PackageDocument::Readme, "1.2.0").unwrap_err();
        assert!(error.contains("HTTP 500"), "{error}");
        server.set(path, Reply::ok(b"not json".to_vec()));
        let error = fetch(&server, PackageDocument::Readme, "1.2.0").unwrap_err();
        assert!(error.contains("invalid"), "{error}");
        server.set(path, Reply::ok(vec![b' '; MAX_DOCUMENT_BYTES + 1]));
        let error = fetch(&server, PackageDocument::Readme, "1.2.0").unwrap_err();
        assert!(error.contains("larger than allowed"), "{error}");
    }
}
