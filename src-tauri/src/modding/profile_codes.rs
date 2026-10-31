//! Thunderstore profile codes: an `.r2z` uploaded as `#r2modman\n` + base64.

use std::time::Duration;

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;

use super::thunderstore_http::{ThunderstoreHttp, describe};
use crate::network::NetworkError;

pub(crate) const DEFAULT_BASE_URL: &str = "https://thunderstore.io";
/// Largest `.r2z` accepted as a code, the same limit as r2modman.
pub(crate) const MAX_CODE_ARCHIVE_BYTES: usize = 20_000_000;
const PAYLOAD_PREFIX: &str = "#r2modman";
const MAX_PAYLOAD_BYTES: usize = MAX_CODE_ARCHIVE_BYTES / 3 * 4 + 1024;
const MAX_RESPONSE_BYTES: usize = MAX_PAYLOAD_BYTES + PAYLOAD_PREFIX.len() + 8;
const MAX_CREATE_RESPONSE_BYTES: usize = 64 * 1024;
const FETCH_ATTEMPTS: u32 = 5;
const FETCH_RETRY_DELAY: Duration = Duration::from_secs(1);

#[derive(Deserialize)]
struct CreateResponse {
    key: String,
}

/// Codes are UUIDs, the same check r2modman applies before importing.
pub(crate) fn is_valid_profile_code(code: &str) -> bool {
    let parts: Vec<&str> = code.split('-').collect();
    parts.len() == 5
        && parts.iter().zip([8, 4, 4, 4, 12]).all(|(part, length)| {
            part.len() == length && part.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
}

pub(crate) fn encode_profile_payload(r2z: &[u8]) -> Result<String, String> {
    if r2z.len() > MAX_CODE_ARCHIVE_BYTES {
        return Err("The profile is too large to be shared as a code".to_owned());
    }
    Ok(format!("{PAYLOAD_PREFIX}\n{}", STANDARD.encode(r2z)))
}

pub(crate) fn decode_profile_payload(payload: &str) -> Result<Vec<u8>, String> {
    let body = payload
        .trim_start()
        .strip_prefix(PAYLOAD_PREFIX)
        .ok_or_else(|| "The profile data is not in the r2modman format".to_owned())?;
    let encoded: String = body.split_whitespace().collect();
    if encoded.len() > MAX_PAYLOAD_BYTES {
        return Err("The profile data is too large".to_owned());
    }
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| "The profile data is corrupted".to_owned())?;
    if bytes.len() > MAX_CODE_ARCHIVE_BYTES {
        return Err("The profile data is too large".to_owned());
    }
    Ok(bytes)
}

fn endpoint(base_url: &str, path: &str) -> String {
    format!(
        "{}/api/experimental/legacyprofile/{path}",
        base_url.trim_end_matches('/')
    )
}

/// Uploads an `.r2z` and returns the profile code.
pub(crate) async fn create_profile_code(
    http: &ThunderstoreHttp,
    base_url: &str,
    r2z: &[u8],
) -> Result<String, String> {
    let payload = encode_profile_payload(r2z)?;
    let body = http
        .try_post_bytes(
            &endpoint(base_url, "create/"),
            "application/octet-stream",
            payload.into_bytes(),
            MAX_CREATE_RESPONSE_BYTES,
        )
        .await
        .map_err(|error| describe_failure(&error, "upload"))?;
    let created: CreateResponse = serde_json::from_slice(&body)
        .map_err(|_| "Thunderstore returned an unexpected response".to_owned())?;
    if !is_valid_profile_code(&created.key) {
        return Err("Thunderstore returned an invalid profile code".to_owned());
    }
    Ok(created.key)
}

/// Downloads a profile code and returns the `.r2z` bytes.
pub(crate) async fn fetch_profile_code(
    http: &ThunderstoreHttp,
    base_url: &str,
    code: &str,
) -> Result<Vec<u8>, String> {
    fetch_with_retry(http, base_url, code, FETCH_RETRY_DELAY).await
}

/// Like r2modman, tries up to five times, and gives up early when the code is
/// unknown or the rate limit was hit.
async fn fetch_with_retry(
    http: &ThunderstoreHttp,
    base_url: &str,
    code: &str,
    delay: Duration,
) -> Result<Vec<u8>, String> {
    let code = code.trim();
    if !is_valid_profile_code(code) {
        return Err("The profile code is not valid".to_owned());
    }
    let url = endpoint(base_url, &format!("get/{code}/"));
    let mut attempt = 1;
    loop {
        match http.try_get_bytes(&url, MAX_RESPONSE_BYTES).await {
            Ok(body) => {
                let payload = String::from_utf8(body)
                    .map_err(|_| "The profile data is not in the r2modman format".to_owned())?;
                return decode_profile_payload(&payload);
            }
            Err(error) => {
                let permanent = matches!(error, NetworkError::Http { status: 404 | 429 });
                if permanent || attempt >= FETCH_ATTEMPTS {
                    return Err(describe_failure(&error, "download"));
                }
                attempt += 1;
                tokio::time::sleep(delay).await;
            }
        }
    }
}

fn describe_failure(error: &NetworkError, action: &str) -> String {
    match error {
        NetworkError::Http { status: 404 } => {
            "The profile code has expired or is invalid".to_owned()
        }
        NetworkError::Http { status: 429 } => {
            "Too many attempts in a short period of time, wait a while and try again".to_owned()
        }
        NetworkError::Http { status } => {
            format!("Thunderstore failed the profile {action} with status {status}")
        }
        other => describe(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        diagnostics::Diagnostics,
        modding::test_support::{Reply, TestServer},
    };

    const CODE: &str = "0b7a6f3e-1c2d-4e5f-8a9b-0123456789ab";
    const CREATE_PATH: &str = "/api/experimental/legacyprofile/create/";

    fn http() -> ThunderstoreHttp {
        ThunderstoreHttp::new(
            "0.1.0",
            Diagnostics::new(Err("test logs disabled".to_owned())),
        )
        .unwrap()
    }

    fn get_path() -> String {
        format!("/api/experimental/legacyprofile/get/{CODE}/")
    }

    async fn fetch(server: &TestServer) -> Result<Vec<u8>, String> {
        fetch_with_retry(&http(), &server.url(""), CODE, Duration::ZERO).await
    }

    #[test]
    fn validates_codes() {
        assert!(is_valid_profile_code(CODE));
        assert!(is_valid_profile_code(&CODE.to_uppercase()));
        for code in [
            "",
            "abc",
            "0b7a6f3e-1c2d-4e5f-8a9b-0123456789a",
            "0b7a6f3e-1c2d-4e5f-8a9b-0123456789abc",
            "0b7a6f3e-1c2d-4e5f-8a9b-0123456789ag",
            "../0b7a6f3e-1c2d-4e5f-8a9b-0123456789",
            "0b7a6f3e1c2d4e5f8a9b0123456789ab",
        ] {
            assert!(!is_valid_profile_code(code), "{code}");
        }
    }

    #[test]
    fn round_trips_payload() {
        let payload = encode_profile_payload(b"zip bytes").unwrap();
        assert!(payload.starts_with("#r2modman\n"));
        assert_eq!(decode_profile_payload(&payload).unwrap(), b"zip bytes");
        let wrapped = payload.replacen('\n', "\r\n", 1) + "\n";
        assert_eq!(decode_profile_payload(&wrapped).unwrap(), b"zip bytes");
    }

    #[test]
    fn rejects_bad_payloads() {
        assert!(decode_profile_payload("UEsDBA==").is_err());
        assert!(decode_profile_payload("#r2modman\n!!!").is_err());
        assert!(encode_profile_payload(&vec![0; MAX_CODE_ARCHIVE_BYTES + 1]).is_err());
        let oversized = format!("#r2modman\n{}", "A".repeat(MAX_PAYLOAD_BYTES + 1));
        assert!(decode_profile_payload(&oversized).is_err());
    }

    #[tokio::test]
    async fn creates_code() {
        let server = TestServer::start();
        server.set(
            CREATE_PATH,
            Reply::ok(format!("{{\"key\":\"{CODE}\"}}").into_bytes()),
        );
        let code = create_profile_code(&http(), &server.url(""), b"zip")
            .await
            .unwrap();
        assert_eq!(code, CODE);
        assert!(server.headers(CREATE_PATH)[0].contains("content-type: application/octet-stream"));
        assert_eq!(
            server.bodies(CREATE_PATH)[0],
            encode_profile_payload(b"zip").unwrap().into_bytes()
        );
    }

    #[tokio::test]
    async fn rejects_invalid_created_key() {
        let server = TestServer::start();
        server.set(CREATE_PATH, Reply::ok(b"{\"key\":\"nope\"}".to_vec()));
        assert!(
            create_profile_code(&http(), &server.url(""), b"zip")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn fetches_code() {
        let server = TestServer::start();
        server.set(
            &get_path(),
            Reply::ok(encode_profile_payload(b"zip").unwrap().into_bytes()),
        );
        assert_eq!(fetch(&server).await.unwrap(), b"zip");
    }

    #[tokio::test]
    async fn stops_retrying_on_expired_code() {
        let server = TestServer::start();
        server.set(&get_path(), Reply::status(404));
        let error = fetch(&server).await.unwrap_err();
        assert!(error.contains("expired or is invalid"));
        assert_eq!(server.requests().len(), 1);
    }

    #[tokio::test]
    async fn stops_retrying_on_rate_limit() {
        let server = TestServer::start();
        server.set(&get_path(), Reply::status(429));
        let error = fetch(&server).await.unwrap_err();
        assert!(error.contains("Too many attempts"));
        assert_eq!(server.requests().len(), 1);
    }

    #[tokio::test]
    async fn retries_other_failures_five_times() {
        let server = TestServer::start();
        server.set(&get_path(), Reply::status(500));
        let error = fetch(&server).await.unwrap_err();
        assert!(error.contains("500"));
        assert_eq!(server.requests().len(), 5);
    }

    #[tokio::test]
    async fn rejects_invalid_codes_without_a_request() {
        let server = TestServer::start();
        let error = fetch_with_retry(&http(), &server.url(""), "bad", Duration::ZERO)
            .await
            .unwrap_err();
        assert!(error.contains("not valid"));
        assert!(server.requests().is_empty());
    }
}
