use std::sync::{Arc, Mutex};

use serde::Serialize;

const CONNECT_TIMEOUT_SECONDS: u64 = 5;
const REQUEST_TIMEOUT_SECONDS: u64 = 10;
const CONNECTIVITY_URL: &str = "https://store.steampowered.com/";

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
}

impl NetworkState {
    pub fn new(version: &str) -> Result<Self, String> {
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
        })
    }
    pub fn status(&self) -> Result<NetworkStatus, String> {
        self.status
            .lock()
            .map(|status| *status)
            .map_err(|_| "network status lock was poisoned".to_owned())
    }

    pub async fn check_connectivity(&self) -> ConnectivityCheck {
        match self.client.get(CONNECTIVITY_URL).send().await {
            Ok(_) => self.update(NetworkStatus::Online, None),
            Err(error) => self.update(
                NetworkStatus::Unknown,
                Some(format!("Steam connectivity check failed: {error}")),
            ),
        }
    }

    fn update(&self, status: NetworkStatus, detail: Option<String>) -> ConnectivityCheck {
        if let Ok(mut current) = self.status.lock() {
            *current = status;
        }
        ConnectivityCheck { status, detail }
    }
}
