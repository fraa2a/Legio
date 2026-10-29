use crate::download_resolver::{Provider, ResolvedDownload, ResolvedFile};
use reqwest::{
    Client, Method, Url,
    header::{HeaderMap, HeaderValue},
};
use serde::{
    Deserialize, Serialize,
    de::{MapAccess, Visitor},
};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        Arc, LazyLock, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex as AsyncMutex;

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";
const LANGUAGE: &str = "en-US";
// The website rotates its token secret server-side and rejects retired ones, so a
// secret must not outlive the window in which the website itself still signs with it.
const SECRET_MAX_AGE: Duration = Duration::from_secs(4 * 60 * 60);
// The obfuscated generator needs ~0.1 s in an optimized build and ~0.7 s in an
// unoptimized one; the limit only has to stop runaway scripts.
const SCRIPT_TIME_LIMIT: Duration = Duration::from_secs(10);
static STATE: OnceLock<AsyncMutex<Session>> = OnceLock::new();
static CACHE_PATH: OnceLock<PathBuf> = OnceLock::new();
static SCRIPT_PATTERN: LazyLock<Option<regex::Regex>> = LazyLock::new(|| {
    regex::Regex::new(r#"(?i)<script[^>]+src=["']([^"']*wt[^"'/]*\.js)["']"#).ok()
});

#[derive(Default)]
struct Session {
    token: Option<String>,
    secret: Option<(String, Instant)>,
}
#[derive(Serialize, Deserialize)]
struct GuestCache {
    version: u8,
    token: String,
    #[serde(rename = "createdAt")]
    created_at: u64,
}
#[derive(Deserialize, Default)]
struct Metadata {
    #[serde(default, rename = "totalPages")]
    total_pages: usize,
}
#[derive(Deserialize)]
struct Reply {
    status: String,
    #[serde(default)]
    data: Box<serde_json::value::RawValue>,
    #[serde(default)]
    metadata: Metadata,
}
#[derive(Deserialize, Debug)]
struct Content {
    #[serde(default)]
    id: String,
    #[serde(rename = "type")]
    kind: String,
    name: Option<String>,
    link: Option<String>,
    #[serde(rename = "canAccess")]
    can_access: Option<bool>,
    #[serde(default)]
    password: bool,
    #[serde(default, rename = "passwordStatus")]
    password_status: Option<String>,
    #[serde(default, rename = "isFrozen")]
    frozen: bool,
    #[serde(default)]
    overloaded: bool,
    #[serde(default, deserialize_with = "children_in_order")]
    children: Vec<Content>,
}

impl Content {
    /// Access gates arrive as successful responses with `canAccess` false.
    fn gate_message(&self) -> &'static str {
        if !self.password {
            "GoFile content is private or expired"
        } else if self.password_status.as_deref() == Some("passwordWrong") {
            "The GoFile password is incorrect"
        } else {
            "GoFile content requires a password"
        }
    }

    fn unavailable_message(&self) -> Option<&'static str> {
        if self.frozen {
            Some("The GoFile file is in cold storage and cannot be downloaded")
        } else if self.overloaded {
            Some("The GoFile server hosting this file is overloaded, try again later")
        } else {
            None
        }
    }
}

fn children_in_order<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Content>, D::Error> {
    struct Children;
    impl<'de> Visitor<'de> for Children {
        type Value = Vec<Content>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("GoFile children map")
        }
        fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
            let mut children = Vec::new();
            while let Some((_, child)) = map.next_entry::<String, Content>()? {
                children.push(child);
            }
            Ok(children)
        }
    }
    deserializer.deserialize_map(Children)
}

pub(crate) fn initialize(directory: PathBuf) {
    let _ = CACHE_PATH.set(directory.join("gofile-guest-token.json"));
}

fn extract_secret(script: &str, time_limit: Duration) -> Result<String, String> {
    let runtime = rquickjs::Runtime::new().map_err(|_| "Cannot create GoFile script runtime")?;
    runtime.set_memory_limit(16 * 1024 * 1024);
    runtime.set_max_stack_size(256 * 1024);
    let deadline = Instant::now() + time_limit;
    let timed_out = Arc::new(AtomicBool::new(false));
    let interrupted = timed_out.clone();
    runtime.set_interrupt_handler(Some(Box::new(move || {
        let expired = Instant::now() >= deadline;
        if expired {
            interrupted.store(true, Ordering::Relaxed);
        }
        expired
    })));
    let context =
        rquickjs::Context::full(&runtime).map_err(|_| "Cannot create GoFile script context")?;
    let raw: String = context
        .with(|ctx| {
            ctx.eval::<(), _>(include_str!("gofile_probe.js"))?;
            ctx.eval::<(), _>(script)?;
            ctx.eval::<String, _>(include_str!("gofile_capture.js"))
        })
        .map_err(|_| {
            if timed_out.load(Ordering::Relaxed) {
                "GoFile website-token generator exceeded its time limit"
            } else {
                "GoFile website-token generator is unsupported or exceeded its memory limit"
            }
        })?;
    let suffix = raw
        .strip_prefix("HydraGofileUserAgent::HydraGofileLanguage::HydraGofileToken::")
        .ok_or("GoFile website-token generator format is unsupported")?;
    let (_, secret) = suffix
        .split_once("::")
        .ok_or("GoFile website-token secret is missing")?;
    let secret = secret.trim();
    if secret.is_empty() || secret.len() > 4096 {
        return Err("GoFile website-token secret is invalid".into());
    }
    Ok(secret.to_owned())
}

fn website_token(secret: &str, token: &str, seconds: u64) -> String {
    let raw = format!(
        "{USER_AGENT}::{LANGUAGE}::{token}::{}::{secret}",
        seconds / 14400
    );
    format!("{:x}", Sha256::digest(raw.as_bytes()))
}
fn content_url(id: &str, page: usize, password: Option<&str>) -> String {
    let mut url = format!(
        "https://api.gofile.io/contents/{id}?page={page}&pageSize=1000&sortField=createTime&sortDirection=-1"
    );
    if let Some(password) = password {
        url.push_str(&format!("&password={}", hash_password(password)));
    }
    url
}
fn hash_password(password: &str) -> String {
    let password = password.trim();
    if password.len() == 64 && password.bytes().all(|c| c.is_ascii_hexdigit()) {
        password.to_ascii_lowercase()
    } else {
        format!("{:x}", Sha256::digest(password.as_bytes()))
    }
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 256
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}
fn valid_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= 4096
        && token
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}
fn headers(token: Option<&str>, secret: &str) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();
    headers.insert("user-agent", HeaderValue::from_static(USER_AGENT));
    headers.insert("origin", HeaderValue::from_static("https://gofile.io"));
    headers.insert("referer", HeaderValue::from_static("https://gofile.io/"));
    headers.insert("accept", HeaderValue::from_static("application/json"));
    headers.insert("x-bl", HeaderValue::from_static(LANGUAGE));
    headers.insert(
        "x-website-token",
        HeaderValue::from_str(&website_token(secret, token.unwrap_or(""), now()))
            .map_err(|_| "Invalid GoFile website token")?,
    );
    if let Some(token) = token {
        headers.insert(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|_| "Invalid GoFile account token")?,
        );
    }
    Ok(headers)
}

async fn response_bytes(mut response: reqwest::Response) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("GoFile network: {}", e.without_url()))?
    {
        if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
            return Err("GoFile response exceeds the size limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
/// Lets the caller renew a rejected guest token without matching message text.
enum GofileError {
    Unauthorized,
    Other(String),
}

impl std::fmt::Display for GofileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthorized => formatter.write_str("GoFile authorization rejected"),
            Self::Other(message) => formatter.write_str(message),
        }
    }
}

impl From<String> for GofileError {
    fn from(error: String) -> Self {
        Self::Other(error)
    }
}

impl From<&str> for GofileError {
    fn from(error: &str) -> Self {
        Self::Other(error.to_owned())
    }
}

impl From<GofileError> for String {
    fn from(error: GofileError) -> Self {
        error.to_string()
    }
}

async fn request(
    client: &Client,
    method: Method,
    url: &str,
    headers: HeaderMap,
) -> Result<Reply, GofileError> {
    for attempt in 0..3 {
        let response = client
            .request(method.clone(), url)
            .headers(headers.clone())
            .timeout(Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| format!("GoFile network: {}", e.without_url()))?;
        let status = response.status();
        let bytes = response_bytes(response).await?;
        let reply = serde_json::from_slice::<Reply>(&bytes);
        let rate_limited = reply.as_ref().is_ok_and(|r| r.status == "error-rateLimit");
        if attempt < 2
            && (rate_limited || matches!(status.as_u16(), 408 | 425 | 429 | 500 | 502 | 503 | 504))
        {
            tokio::time::sleep(Duration::from_secs(1 << attempt)).await;
            continue;
        }
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(GofileError::Unauthorized);
        }
        if rate_limited || status.as_u16() == 429 {
            return Err("GoFile rate limit reached; retry later".into());
        }
        let reply = reply.map_err(|_| {
            format!(
                "GoFile returned an invalid API response (HTTP {})",
                status.as_u16()
            )
        })?;
        if !status.is_success() || reply.status != "ok" {
            return Err(api_error(&reply.status));
        }
        return Ok(reply);
    }
    Err("GoFile retry limit reached".into())
}
fn api_error(status: &str) -> GofileError {
    match status {
        "error-wrongToken" | "error-notAuthenticated" | "error-badToken" | "error-notPremium" => {
            GofileError::Unauthorized
        }
        "error-notFound" => "GoFile content was not found".into(),
        "error-passwordRequired" | "error-passwordWrong" => {
            "GoFile content requires a valid password".into()
        }
        _ => "GoFile denied the requested content".into(),
    }
}

fn script_url(html: &str) -> Option<String> {
    let path = SCRIPT_PATTERN.as_ref()?.captures(html)?.get(1)?.as_str();
    let url = Url::parse("https://gofile.io/").ok()?.join(path).ok()?;
    let host = url.host_str()?;
    (url.scheme() == "https"
        && (host == "gofile.io" || host.ends_with(".gofile.io"))
        && url.username().is_empty()
        && url.password().is_none())
    .then(|| url.to_string())
}
async fn website_text(client: &Client, url: &str) -> Result<String, String> {
    let mut headers = HeaderMap::new();
    headers.insert("user-agent", HeaderValue::from_static(USER_AGENT));
    headers.insert("referer", HeaderValue::from_static("https://gofile.io/"));
    // Do not execute script bodies delivered through a cross-origin redirect.
    let response = crate::download_resolver::get(client, url, headers).await?;
    let host = response.url().host_str().unwrap_or_default();
    if response.url().scheme() != "https" || !(host == "gofile.io" || host.ends_with(".gofile.io"))
    {
        return Err("GoFile script redirected outside its website".into());
    }
    String::from_utf8(response_bytes(response).await?)
        .map_err(|_| "GoFile returned invalid text".into())
}
async fn secret(client: &Client) -> Result<String, String> {
    let mut urls = Vec::new();
    if let Ok(html) = website_text(client, "https://gofile.io/").await
        && let Some(url) = script_url(&html)
    {
        urls.push(url);
    }
    urls.extend([
        "https://gofile.io/js/wt.obf.js".into(),
        "https://gofile.io/dist/js/wt.obf.js".into(),
    ]);
    urls.dedup();
    // Keep the first failure: later candidates are fallback paths that usually just 404.
    let mut first_error = None;
    for url in urls {
        let script = match website_text(client, &url).await {
            Ok(script) if script.trim_start().starts_with('<') => {
                first_error.get_or_insert_with(|| "the script URL returned a web page".to_owned());
                continue;
            }
            Ok(script) => script,
            Err(error) => {
                first_error.get_or_insert(error);
                continue;
            }
        };
        let result =
            tokio::task::spawn_blocking(move || extract_secret(&script, SCRIPT_TIME_LIMIT))
                .await
                .map_err(|_| "GoFile script worker failed")?;
        match result {
            Ok(secret) => return Ok(secret),
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }
    let reason = first_error.unwrap_or_else(|| "no script URL was available".to_owned());
    crate::application_log::failure("gofile_website_token", &reason);
    Err(format!(
        "Cannot load the current GoFile website-token generator: {reason}"
    ))
}
async fn load_token() -> Option<String> {
    let path = CACHE_PATH.get()?;
    let metadata = tokio::fs::metadata(path).await.ok()?;
    if metadata.len() > 8192 {
        return None;
    }
    let bytes = tokio::fs::read(path).await.ok()?;
    let cache: GuestCache = serde_json::from_slice(&bytes).ok()?;
    (cache.version == 1 && valid_token(&cache.token)).then_some(cache.token)
}
async fn persist_token(token: &str) {
    let Some(path) = CACHE_PATH.get() else {
        return;
    };
    let cache = GuestCache {
        version: 1,
        token: token.into(),
        created_at: now() * 1000,
    };
    let result = async {
        let parent = path
            .parent()
            .ok_or("GoFile cache directory is unavailable".to_owned())?;
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
        let temporary = path.with_extension("tmp");
        let bytes = serde_json::to_vec(&cache).map_err(|e| e.to_string())?;
        let mut options = tokio::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        options.mode(0o600);
        use tokio::io::AsyncWriteExt;
        let mut file = options.open(&temporary).await.map_err(|e| e.to_string())?;
        file.write_all(&bytes).await.map_err(|e| e.to_string())?;
        file.flush().await.map_err(|e| e.to_string())?;
        drop(file);
        tokio::fs::rename(temporary, path)
            .await
            .map_err(|e| e.to_string())
    }
    .await;
    if let Err(error) = result {
        crate::application_log::failure("gofile_guest_cache", &error);
    }
}
async fn create_token(client: &Client, secret: &str) -> Result<String, String> {
    let reply = request(
        client,
        Method::POST,
        "https://api.gofile.io/accounts",
        headers(None, secret)?,
    )
    .await?;
    let data: serde_json::Value =
        serde_json::from_str(reply.data.get()).map_err(|_| "Invalid GoFile account response")?;
    let token = data
        .get("token")
        .and_then(|v| v.as_str())
        .filter(|t| valid_token(t))
        .ok_or("GoFile guest token is unavailable")?;
    persist_token(token).await;
    Ok(token.to_owned())
}

async fn first_file(
    client: &Client,
    id: &str,
    token: &str,
    secret: &str,
    password: Option<&str>,
) -> Result<ResolvedFile, GofileError> {
    enum Work {
        Lookup(String),
        Item(Content),
    }
    let mut work = vec![Work::Lookup(id.into())];
    let mut visited = HashSet::new();
    let mut unavailable = None;
    let mut requests = 0;
    while let Some(item) = work.pop() {
        match item {
            Work::Lookup(id) => {
                if !valid_id(&id) {
                    return Err("GoFile returned an invalid folder ID".into());
                }
                if !visited.insert(id.clone()) {
                    continue;
                }
                let mut page = 1;
                let mut pages = Vec::new();
                loop {
                    requests += 1;
                    if requests > 100 {
                        return Err("GoFile folder exceeds the lookup limit".into());
                    }
                    let url = content_url(&id, page, password);
                    let reply =
                        request(client, Method::GET, &url, headers(Some(token), secret)?).await?;
                    let content: Content = serde_json::from_str(reply.data.get())
                        .map_err(|_| "GoFile content response is invalid")?;
                    if content.can_access == Some(false) {
                        return Err(content.gate_message().into());
                    }
                    pages.push(content);
                    if page >= reply.metadata.total_pages.max(1) {
                        break;
                    }
                    page += 1;
                }
                work.extend(pages.into_iter().rev().map(Work::Item));
            }
            Work::Item(content) => {
                if content.kind == "file" {
                    if let Some(message) = content.unavailable_message() {
                        unavailable.get_or_insert(message);
                        continue;
                    }
                    if content.link.is_none() {
                        continue;
                    }
                    let value = serde_json::json!({"name": content.name.unwrap_or_else(|| "download".into()), "link": content.link, "canAccess": content.can_access});
                    let (name, url) = crate::download_resolver::gofile_files(&value)?.remove(0);
                    let mut headers = HeaderMap::new();
                    headers.insert(
                        "cookie",
                        HeaderValue::from_str(&format!("accountToken={token}"))
                            .map_err(|_| "Invalid GoFile cookie")?,
                    );
                    headers.insert("user-agent", HeaderValue::from_static(USER_AGENT));
                    return Ok(ResolvedFile {
                        name: Some(name),
                        url,
                        headers,
                    });
                }
                if content.kind != "folder" {
                    return Err("GoFile returned an unsupported content type".into());
                }
                if content.children.len() > 1000 {
                    return Err("GoFile page has too many files".into());
                }
                for child in content.children.into_iter().rev() {
                    if child.can_access == Some(false) {
                        continue;
                    }
                    work.push(if child.kind == "folder" {
                        Work::Lookup(child.id)
                    } else {
                        Work::Item(child)
                    });
                }
            }
        }
    }
    Err(unavailable
        .unwrap_or("GoFile folder has no accessible file links")
        .into())
}
async fn official(
    client: &Client,
    id: &str,
    password: Option<&str>,
) -> Result<ResolvedFile, String> {
    let mut state = STATE
        .get_or_init(|| AsyncMutex::new(Session::default()))
        .lock()
        .await;
    if state
        .secret
        .as_ref()
        .is_none_or(|(_, loaded)| loaded.elapsed() >= SECRET_MAX_AGE)
    {
        state.secret = Some((secret(client).await?, Instant::now()));
    }
    let current_secret = state
        .secret
        .as_ref()
        .map(|(secret, _)| secret.clone())
        .ok_or("GoFile website token is unavailable")?;
    if state.token.is_none() {
        state.token = load_token().await;
    }
    if state.token.is_none() {
        match create_token(client, &current_secret).await {
            Ok(token) => state.token = Some(token),
            Err(error) => {
                // A rotated website-token generator makes the cached secret stale.
                state.secret = None;
                return Err(error);
            }
        }
    }
    let token = state
        .token
        .as_deref()
        .ok_or("GoFile guest token is unavailable")?;
    let file = match first_file(client, id, token, &current_secret, password).await {
        Err(GofileError::Unauthorized) => {
            state.token = None;
            state.secret = None;
            let fresh_secret = secret(client).await?;
            let token = create_token(client, &fresh_secret).await?;
            state.secret = Some((fresh_secret.clone(), Instant::now()));
            state.token = Some(token.clone());
            first_file(client, id, &token, &fresh_secret, password).await
        }
        result => result,
    }?;
    crate::download_resolver::head(client, &file.url, file.headers.clone()).await?;
    Ok(file)
}

pub(crate) async fn resolve(
    client: &Client,
    id: &str,
    password: Option<&str>,
) -> Result<ResolvedDownload, String> {
    if !valid_id(id) {
        return Err("Invalid GoFile content ID".into());
    }
    let file = tokio::time::timeout(Duration::from_secs(120), official(client, id, password))
        .await
        .map_err(|_| "GoFile resolution timed out".to_owned())??;
    Ok(ResolvedDownload {
        provider: Provider::Gofile,
        files: vec![file],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_secret_by_running_the_website_generator() {
        let script = include_str!("../../scripts/fixtures/gofile-wt.js");
        assert_eq!(
            extract_secret(script, SCRIPT_TIME_LIMIT).unwrap(),
            "secret::suffix"
        );
    }

    #[test]
    fn website_token_uses_hydras_identity_and_four_hour_slot() {
        let expected = "d92fe0fb05b6f1d52aee5023c124dbbe823e637a4b5032cd9ecacf40a862231d";
        assert_eq!(website_token("secret", "guest", 28800), expected);
        assert_eq!(website_token("secret", "guest", 43199), expected);
        assert_ne!(website_token("secret", "guest", 43200), expected);
    }

    #[test]
    fn content_urls_hash_passwords_and_preserve_existing_hashes() {
        let hashed = format!("{:x}", Sha256::digest(b"test password"));
        let url = content_url("folder", 2, Some(" test password "));
        assert!(url.contains("page=2&pageSize=1000&sortField=createTime&sortDirection=-1"));
        assert!(url.ends_with(&format!("&password={hashed}")));
        assert_eq!(content_url("folder", 2, Some(&hashed.to_uppercase())), url);
        assert!(!content_url("folder", 1, None).contains("password="));
    }

    #[test]
    fn script_discovery_rejects_external_and_insecure_origins() {
        assert_eq!(
            script_url(r#"<script src="/dist/js/wt.obf.js"></script>"#).as_deref(),
            Some("https://gofile.io/dist/js/wt.obf.js")
        );
        assert!(
            script_url(r#"<script src="https://gofile.io.attacker.test/wt.js"></script>"#)
                .is_none()
        );
        assert!(script_url(r#"<script src="http://gofile.io/wt.js"></script>"#).is_none());
    }

    #[test]
    fn api_decoding_preserves_website_file_order() {
        let reply: Reply = serde_json::from_str(r#"{"status":"ok","data":{"id":"root","type":"folder","children":{"z":{"id":"z","type":"file"},"a":{"id":"a","type":"file"}}},"metadata":{"totalPages":2}}"#).unwrap();
        let content: Content = serde_json::from_str(reply.data.get()).unwrap();
        assert_eq!(
            content
                .children
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            vec!["z", "a"]
        );
        assert_eq!(reply.metadata.total_pages, 2);
    }

    #[test]
    fn rejects_unrecognized_generators_and_infinite_scripts() {
        let limit = Duration::from_millis(200);
        assert!(extract_secret("function generateWT() { return 'invalid'; }", limit).is_err());
        assert!(extract_secret("require('node:fs').readFileSync('/etc/passwd')", limit).is_err());
        assert_eq!(
            extract_secret("while (true) {}", limit).unwrap_err(),
            "GoFile website-token generator exceeded its time limit"
        );
    }

    #[test]
    fn gates_report_why_content_is_not_accessible() {
        let gate = |data: &str| {
            let reply: Reply =
                serde_json::from_str(&format!(r#"{{"status":"ok","data":{data}}}"#)).unwrap();
            let content: Content = serde_json::from_str(reply.data.get()).unwrap();
            assert_eq!(content.can_access, Some(false));
            content.gate_message()
        };
        assert_eq!(
            gate(
                r#"{"canAccess":false,"id":"a","type":"folder","password":true,"passwordStatus":"passwordRequired"}"#
            ),
            "GoFile content requires a password"
        );
        assert_eq!(
            gate(
                r#"{"canAccess":false,"id":"a","type":"folder","password":true,"passwordStatus":"passwordWrong"}"#
            ),
            "The GoFile password is incorrect"
        );
        assert_eq!(
            gate(r#"{"canAccess":false,"id":"a","type":"folder","public":false}"#),
            "GoFile content is private or expired"
        );
    }

    #[test]
    fn frozen_and_overloaded_files_are_reported_instead_of_downloaded() {
        let file = |flags: &str| -> Content {
            serde_json::from_str(&format!(
                r#"{{"id":"f","type":"file","name":"game.zip","link":"https://store1.gofile.io/download/web/f/game.zip"{flags}}}"#
            ))
            .unwrap()
        };
        assert_eq!(file("").unavailable_message(), None);
        assert!(file(r#","isFrozen":true"#).unavailable_message().is_some());
        assert!(
            file(r#","overloaded":true"#)
                .unavailable_message()
                .is_some()
        );
    }

    #[test]
    fn decodes_a_single_file_share() {
        // A share code that points at one file answers with the file itself and empty metadata.
        let reply: Reply = serde_json::from_str(r#"{"status":"ok","data":{"canAccess":true,"id":"1e5a5690-0000-4000-8000-000000000000","type":"file","name":"game.zip","code":"AbCdEf","size":1024,"servers":["store-na-phx-1"],"link":"https://store-na-phx-1.gofile.io/download/web/1e5a5690-0000-4000-8000-000000000000/game.zip","noAds":true},"metadata":{}}"#).unwrap();
        let content: Content = serde_json::from_str(reply.data.get()).unwrap();
        assert_eq!(content.kind, "file");
        assert_eq!(reply.metadata.total_pages, 0);
        let value = serde_json::json!({"name": content.name, "link": content.link, "canAccess": content.can_access});
        let files = crate::download_resolver::gofile_files(&value).unwrap();
        assert_eq!(files[0].0, "game.zip");
    }
}
