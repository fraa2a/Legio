use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::diagnostics::{Diagnostics, Operation, RequestLog};

const CONNECT_TIMEOUT_SECONDS: u64 = 5;
const REQUEST_TIMEOUT_SECONDS: u64 = 10;
const CONNECTIVITY_URL: &str = "https://store.steampowered.com/";
const HYDRA_SEARCH_URL: &str = "https://hydra-api-us-east-1.losbroxas.org/catalogue/search";
const MAX_CATALOG_BYTES: usize = 2 * 1024 * 1024;
const STEAM_DETAILS_URL: &str = "https://store.steampowered.com/api/appdetails";

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NetworkError {
    Timeout,
    Transport { message: String },
    Http { status: u16 },
    TooLarge,
}

impl From<reqwest::Error> for NetworkError {
    fn from(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            Self::Timeout
        } else {
            Self::Transport {
                message: error.without_url().to_string(),
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NetworkStatus {
    Unknown,
    Online,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectivityCheck {
    pub status: NetworkStatus,
    pub detail: Option<String>,
}
#[derive(Clone)]
pub struct NetworkState {
    client: reqwest::Client,
    status: Arc<Mutex<NetworkStatus>>,
    diagnostics: Diagnostics,
}

impl NetworkState {
    pub fn new(version: &str, diagnostics: Diagnostics) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(CONNECT_TIMEOUT_SECONDS))
            .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
            .redirect(reqwest::redirect::Policy::limited(3))
            .user_agent(format!("Legio/{version}"))
            .build()
            .map_err(|error| format!("could not initialize the network client: {error}"))?;
        Ok(Self {
            client,
            status: Arc::new(Mutex::new(NetworkStatus::Unknown)),
            diagnostics,
        })
    }
    pub fn status(&self) -> Result<NetworkStatus, String> {
        self.status
            .lock()
            .map(|status| *status)
            .map_err(|_| "network status lock was poisoned".to_owned())
    }

    pub async fn hydra_search(&self, body: Vec<u8>) -> Result<Vec<u8>, NetworkError> {
        self.post_catalog(HYDRA_SEARCH_URL, body).await
    }

    pub async fn steam_details(&self, app_id: u32) -> Result<Vec<u8>, NetworkError> {
        self.get_steam_details(STEAM_DETAILS_URL, app_id).await
    }

    async fn get_steam_details(&self, url: &str, app_id: u32) -> Result<Vec<u8>, NetworkError> {
        let mut log = self.diagnostics.request(Operation::SteamDetails);
        let result = async {
            let response = self
                .client
                .get(format!("{url}?appids={app_id}&l=english"))
                .send()
                .await?;
            Self::read_bounded(response, &mut log).await
        }
        .await;
        log.finish(&result);
        result
    }

    // Dropping this future cancels the request and body read without background work.
    async fn post_catalog(&self, url: &str, body: Vec<u8>) -> Result<Vec<u8>, NetworkError> {
        let mut log = self.diagnostics.request(Operation::HydraSearch);
        let result = async {
            let response = self
                .client
                .post(url)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body)
                .send()
                .await?;
            Self::read_bounded(response, &mut log).await
        }
        .await;
        log.finish(&result);
        result
    }

    async fn read_bounded(
        mut response: reqwest::Response,
        log: &mut RequestLog<'_>,
    ) -> Result<Vec<u8>, NetworkError> {
        log.set_status(response.status().as_u16());
        if !response.status().is_success() {
            return Err(NetworkError::Http {
                status: response.status().as_u16(),
            });
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_CATALOG_BYTES as u64)
        {
            return Err(NetworkError::TooLarge);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            log.set_response_bytes(bytes.len().saturating_add(chunk.len()));
            if chunk.len() > MAX_CATALOG_BYTES - bytes.len() {
                return Err(NetworkError::TooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        log.set_response_bytes(bytes.len());
        Ok(bytes)
    }

    pub async fn check_connectivity(&self) -> ConnectivityCheck {
        self.check_connectivity_at(CONNECTIVITY_URL).await
    }

    async fn check_connectivity_at(&self, url: &str) -> ConnectivityCheck {
        let mut log = self.diagnostics.request(Operation::SteamConnectivity);
        let result = self
            .client
            .get(url)
            .send()
            .await
            .map_err(NetworkError::from)
            .and_then(|response| {
                log.set_status(response.status().as_u16());
                if response.status().is_success() {
                    Ok(())
                } else {
                    Err(NetworkError::Http {
                        status: response.status().as_u16(),
                    })
                }
            });
        log.finish(&result);
        match result {
            Ok(()) => self.update(NetworkStatus::Online, None),
            Err(error) => {
                let detail = match error {
                    NetworkError::Timeout => "Steam connectivity check timed out.".to_owned(),
                    NetworkError::Transport { message } => {
                        format!("Could not reach Steam: {message}")
                    }
                    NetworkError::Http { status } => {
                        format!("Steam connectivity check returned HTTP {status}.")
                    }
                    NetworkError::TooLarge => {
                        "Steam connectivity response was too large.".to_owned()
                    }
                };
                self.update(NetworkStatus::Unknown, Some(detail))
            }
        }
    }

    fn update(&self, status: NetworkStatus, detail: Option<String>) -> ConnectivityCheck {
        if let Ok(mut current) = self.status.lock() {
            *current = status;
        }
        ConnectivityCheck { status, detail }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
        thread,
        time::Duration,
    };

    fn state() -> NetworkState {
        NetworkState::new(
            "0.1.0",
            Diagnostics::new(Err("test logs disabled".to_owned())),
        )
        .unwrap()
    }

    fn logged_state() -> (NetworkState, std::path::PathBuf) {
        let directory =
            std::env::temp_dir().join(format!("legio-network-test-{}", uuid::Uuid::new_v4()));
        let state = NetworkState::new("0.1.0", Diagnostics::new(Ok(directory.clone()))).unwrap();
        (state, directory)
    }

    fn read_log(state: &NetworkState, directory: &std::path::Path) -> String {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while state.diagnostics.status().pending_records != 0 {
            assert!(
                std::time::Instant::now() < deadline,
                "log writer did not finish"
            );
            thread::sleep(Duration::from_millis(5));
        }
        std::fs::read_to_string(directory.join("network.jsonl")).unwrap()
    }

    fn server(response: Vec<u8>) -> (String, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/catalogue/search", listener.local_addr().unwrap());
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
            let request = String::from_utf8(request).unwrap();
            let length: usize = request
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| "0".to_owned())
                .parse()
                .unwrap();
            let mut body = vec![0; length];
            stream.read_exact(&mut body).unwrap();
            // Early rejection can close the socket before all oversized bytes are sent.
            let _ = stream.write_all(&response);
            request + &String::from_utf8(body).unwrap()
        });
        (url, thread)
    }

    #[test]
    fn steam_details_requests_one_public_app_without_credentials() {
        let (url, server) =
            server(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_vec());
        let state = state();
        let bytes = tauri::async_runtime::block_on(state.get_steam_details(&url, 400)).unwrap();
        assert_eq!(bytes, b"{}");
        let request = server.join().unwrap();
        assert!(request.starts_with("GET /catalogue/search?appids=400&l=english HTTP/1.1"));
        assert!(!request.to_ascii_lowercase().contains("authorization:"));
        assert!(!request.to_ascii_lowercase().contains("cookie:"));
    }

    #[test]
    fn sends_anonymous_json_with_truthful_user_agent() {
        let (url, server) =
            server(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_vec());
        let (state, directory) = logged_state();
        let bytes = tauri::async_runtime::block_on(
            state.post_catalog(&url, br#"{"title":"Portal","take":50,"skip":0}"#.to_vec()),
        )
        .unwrap();
        assert_eq!(bytes, b"{}");
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /catalogue/search HTTP/1.1"));
        assert!(request.contains("user-agent: Legio/0.1.0\r\n"));
        assert!(!request.to_ascii_lowercase().contains("authorization:"));
        assert!(!request.to_ascii_lowercase().contains("cookie:"));
        assert!(request.ends_with(r#"{"title":"Portal","take":50,"skip":0}"#));
        let log = read_log(&state, &directory);
        let record: serde_json::Value = serde_json::from_str(log.trim()).unwrap();
        assert_eq!(record["operation"], "hydra_search");
        assert_eq!(record["outcome"], "success");
        assert_eq!(record["status"], 200);
        assert_eq!(record["response_bytes"], 2);
        assert!(!log.contains("Portal"));
        assert!(!log.contains("catalogue/search"));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_http_errors_and_both_declared_and_streamed_oversize() {
        let mut chunked =
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n".to_vec();
        chunked.extend_from_slice(format!("{:x}\r\n", MAX_CATALOG_BYTES + 1).as_bytes());
        chunked.resize(chunked.len() + MAX_CATALOG_BYTES + 1, b'x');
        chunked.extend_from_slice(b"\r\n0\r\n\r\n");
        for (response, expected) in [
            (
                b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\n\r\n".to_vec(),
                NetworkError::Http { status: 429 },
            ),
            (
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                    MAX_CATALOG_BYTES + 1
                )
                .into_bytes(),
                NetworkError::TooLarge,
            ),
            (chunked, NetworkError::TooLarge),
        ] {
            let (url, server) = server(response);
            let (state, directory) = logged_state();
            assert_eq!(
                tauri::async_runtime::block_on(state.post_catalog(&url, b"{}".to_vec()))
                    .unwrap_err(),
                expected
            );
            server.join().unwrap();
            let log = read_log(&state, &directory);
            let record: serde_json::Value = serde_json::from_str(log.trim()).unwrap();
            assert_eq!(
                record["status"],
                if matches!(&expected, NetworkError::Http { .. }) {
                    429
                } else {
                    200
                }
            );
            assert_eq!(
                record["outcome"],
                if matches!(&expected, NetworkError::Http { .. }) {
                    "http"
                } else {
                    "too_large"
                }
            );
            std::fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn dropping_request_future_cancels_pending_response() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/catalogue/search", listener.local_addr().unwrap());
        let (received, wait_received) = mpsc::channel();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut buffer = [0; 4096];
            assert!(stream.read(&mut buffer).unwrap() > 0);
            received.send(()).unwrap();
            loop {
                match stream.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(_) => continue,
                    Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => break,
                    Err(error) => panic!("cancelled request kept connection open: {error}"),
                }
            }
        });
        let (state, directory) = logged_state();
        let diagnostics = state.diagnostics.clone();
        let task =
            tauri::async_runtime::spawn(
                async move { state.post_catalog(&url, b"{}".to_vec()).await },
            );
        wait_received.recv_timeout(Duration::from_secs(3)).unwrap();
        task.abort();
        assert!(tauri::async_runtime::block_on(task).is_err());
        server.join().unwrap();
        let state = NetworkState::new("0.1.0", diagnostics).unwrap();
        let log = read_log(&state, &directory);
        let records = log
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["outcome"], "cancelled");
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn classifies_timeout_and_connection_failure() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/catalogue/search", listener.local_addr().unwrap());
        let (release, wait_release) = mpsc::channel();
        let server = thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            wait_release.recv_timeout(Duration::from_secs(3)).unwrap();
        });
        let (mut state, directory) = logged_state();
        state.client = reqwest::Client::builder()
            .timeout(Duration::from_millis(50))
            .build()
            .unwrap();
        assert_eq!(
            tauri::async_runtime::block_on(state.post_catalog(&url, b"{}".to_vec())).unwrap_err(),
            NetworkError::Timeout
        );
        release.send(()).unwrap();
        server.join().unwrap();
        assert!(matches!(
            tauri::async_runtime::block_on(state.post_catalog(&url, b"{}".to_vec())),
            Err(NetworkError::Transport { .. })
        ));
        let log = read_log(&state, &directory);
        let outcomes = log
            .lines()
            .map(|line| {
                serde_json::from_str::<serde_json::Value>(line).unwrap()["outcome"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect::<Vec<_>>();
        assert_eq!(outcomes, ["timeout", "transport"]);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn connectivity_http_failure_stays_unknown_and_records_status() {
        let (url, server) = server(
            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_vec(),
        );
        let (state, directory) = logged_state();
        let check = tauri::async_runtime::block_on(state.check_connectivity_at(&url));
        server.join().unwrap();
        assert_eq!(check.status, NetworkStatus::Unknown);
        let log = read_log(&state, &directory);
        let record: serde_json::Value = serde_json::from_str(log.trim()).unwrap();
        assert_eq!(record["operation"], "steam_connectivity");
        assert_eq!(record["outcome"], "http");
        assert_eq!(record["status"], 503);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
