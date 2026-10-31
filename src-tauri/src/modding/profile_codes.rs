//! Thunderstore profile codes: an `.r2z` uploaded as `#r2modman\n` + base64.

use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::{Client, Response, StatusCode};
use serde::Deserialize;

pub(crate) const DEFAULT_BASE_URL: &str = "https://thunderstore.io";
/// Largest `.r2z` accepted as a code, the same limit as r2modman.
pub(crate) const MAX_CODE_ARCHIVE_BYTES: usize = 20_000_000;
const PAYLOAD_PREFIX: &str = "#r2modman";
const MAX_PAYLOAD_BYTES: usize = MAX_CODE_ARCHIVE_BYTES / 3 * 4 + 1024;
const MAX_DETAIL_BYTES: usize = 64 * 1024;

#[derive(Deserialize)]
struct CreateResponse {
    key: String,
}

#[derive(Deserialize)]
struct ErrorBody {
    detail: Option<String>,
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
    client: &Client,
    base_url: &str,
    r2z: &[u8],
) -> Result<String, String> {
    let payload = encode_profile_payload(r2z)?;
    let response = client
        .post(endpoint(base_url, "create/"))
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .body(payload)
        .send()
        .await
        .map_err(|error| format!("Could not reach Thunderstore: {error}"))?;
    let response = check_status(response, "upload").await?;
    let body = read_limited(response, MAX_DETAIL_BYTES).await?;
    let created: CreateResponse = serde_json::from_slice(&body)
        .map_err(|_| "Thunderstore returned an unexpected response".to_owned())?;
    if !is_valid_profile_code(&created.key) {
        return Err("Thunderstore returned an invalid profile code".to_owned());
    }
    Ok(created.key)
}

/// Downloads a profile code and returns the `.r2z` bytes.
pub(crate) async fn fetch_profile_code(
    client: &Client,
    base_url: &str,
    code: &str,
) -> Result<Vec<u8>, String> {
    let code = code.trim();
    if !is_valid_profile_code(code) {
        return Err("The profile code is not valid".to_owned());
    }
    let response = client
        .get(endpoint(base_url, &format!("get/{code}/")))
        .send()
        .await
        .map_err(|error| format!("Could not reach Thunderstore: {error}"))?;
    let response = check_status(response, "download").await?;
    let body = read_limited(response, MAX_PAYLOAD_BYTES + PAYLOAD_PREFIX.len() + 8).await?;
    let payload = String::from_utf8(body)
        .map_err(|_| "The profile data is not in the r2modman format".to_owned())?;
    decode_profile_payload(&payload)
}

async fn check_status(response: Response, action: &str) -> Result<Response, String> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    match status {
        StatusCode::NOT_FOUND => Err("The profile code has expired or is invalid".to_owned()),
        StatusCode::TOO_MANY_REQUESTS => {
            let detail = read_limited(response, MAX_DETAIL_BYTES)
                .await
                .ok()
                .and_then(|body| serde_json::from_slice::<ErrorBody>(&body).ok())
                .and_then(|body| body.detail);
            Err(detail.unwrap_or_else(|| {
                "Too many attempts in a short period of time, wait a while and try again".to_owned()
            }))
        }
        _ => Err(format!(
            "Thunderstore failed the profile {action} with status {}",
            status.as_u16()
        )),
    }
}

async fn read_limited(mut response: Response, limit: usize) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("Could not read the Thunderstore response: {error}"))?
    {
        if body.len() + chunk.len() > limit {
            return Err("The Thunderstore response is too large".to_owned());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::Duration,
    };

    const CODE: &str = "0b7a6f3e-1c2d-4e5f-8a9b-0123456789ab";

    fn server(status: &str, body: Vec<u8>) -> (String, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let status = status.to_owned();
        let thread = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            let head = String::from_utf8(request).unwrap();
            let length: usize = head
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|value| value.parse().ok())
                })
                .unwrap_or(0);
            let mut request_body = vec![0; length];
            stream.read_exact(&mut request_body).unwrap();
            let mut response = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .into_bytes();
            response.extend_from_slice(&body);
            let _ = stream.write_all(&response);
            head + &String::from_utf8_lossy(&request_body)
        });
        (base, thread)
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
        let (base, request) = server("200 OK", format!("{{\"key\":\"{CODE}\"}}").into_bytes());
        let code = create_profile_code(&Client::new(), &base, b"zip")
            .await
            .unwrap();
        assert_eq!(code, CODE);
        let request = request.join().unwrap();
        assert!(request.starts_with("POST /api/experimental/legacyprofile/create/ "));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("content-type: application/octet-stream")
        );
        assert!(request.ends_with(&encode_profile_payload(b"zip").unwrap()));
    }

    #[tokio::test]
    async fn rejects_invalid_created_key() {
        let (base, _request) = server("200 OK", b"{\"key\":\"nope\"}".to_vec());
        assert!(
            create_profile_code(&Client::new(), &base, b"zip")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn fetches_code() {
        let body = encode_profile_payload(b"zip").unwrap().into_bytes();
        let (base, request) = server("200 OK", body);
        let bytes = fetch_profile_code(&Client::new(), &base, CODE)
            .await
            .unwrap();
        assert_eq!(bytes, b"zip");
        let request = request.join().unwrap();
        assert!(request.starts_with(&format!("GET /api/experimental/legacyprofile/get/{CODE}/ ")));
    }

    #[tokio::test]
    async fn reports_expired_code() {
        let (base, _request) = server("404 Not Found", b"{}".to_vec());
        let error = fetch_profile_code(&Client::new(), &base, CODE)
            .await
            .unwrap_err();
        assert!(error.contains("expired or is invalid"));
    }

    #[tokio::test]
    async fn reports_rate_limit_detail() {
        let (base, _request) = server(
            "429 Too Many Requests",
            b"{\"detail\":\"Request was throttled. Expected available in 30 seconds.\"}".to_vec(),
        );
        let error = fetch_profile_code(&Client::new(), &base, CODE)
            .await
            .unwrap_err();
        assert_eq!(
            error,
            "Request was throttled. Expected available in 30 seconds."
        );
    }

    #[tokio::test]
    async fn reports_other_failures_and_invalid_codes() {
        let (base, _request) = server("500 Internal Server Error", Vec::new());
        let error = fetch_profile_code(&Client::new(), &base, CODE)
            .await
            .unwrap_err();
        assert!(error.contains("500"));
        assert!(
            fetch_profile_code(&Client::new(), "http://127.0.0.1:1", "bad")
                .await
                .unwrap_err()
                .contains("not valid")
        );
    }
}
