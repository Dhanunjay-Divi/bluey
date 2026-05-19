//! Device-flow login (the daemon-side counterpart to
//! `POST /auth/device/{start,poll}` on the server).

use std::time::Duration;

use crate::{
    client::CloudClient,
    error::{Error, Result},
    types::{AuthResponse, DeviceStartResponse},
};

#[derive(Debug, Clone)]
pub enum DeviceFlowState {
    /// The customer has not yet entered the user_code in their browser.
    Pending,
    /// Logged in. Tokens have been saved to the configured store.
    LoggedIn(AuthResponse),
    /// The device_code expired before the customer approved.
    Expired,
}

/// High-level device-flow driver. Steps:
///
///   1. `DeviceFlow::start(&client)` → returns the `user_code` to print
///      and the `verification_uri` to send the customer to.
///   2. `flow.poll(&client)` → call repeatedly until LoggedIn / Expired.
pub struct DeviceFlow {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub interval_secs: u64,
    pub deadline: std::time::Instant,
}

impl DeviceFlow {
    pub async fn start(client: &CloudClient) -> Result<Self> {
        let resp: DeviceStartResponse = client.post_json("/auth/device/start", &()).await?;
        let interval_secs = resp.interval.max(1) as u64;
        let deadline =
            std::time::Instant::now() + Duration::from_secs(resp.expires_in.max(1) as u64);
        Ok(Self {
            device_code: resp.device_code,
            user_code: resp.user_code,
            verification_uri: resp.verification_uri,
            interval_secs,
            deadline,
        })
    }

    /// Poll once. Returns LoggedIn / Pending / Expired without retry.
    pub async fn poll(&self, client: &CloudClient) -> Result<DeviceFlowState> {
        if std::time::Instant::now() >= self.deadline {
            return Ok(DeviceFlowState::Expired);
        }
        let body = serde_json::json!({ "device_code": self.device_code });
        let resp = client.raw_post("/auth/device/poll", &body).await?;
        match resp.status() {
            reqwest::StatusCode::OK => {
                let auth: AuthResponse = resp.json().await?;
                Ok(DeviceFlowState::LoggedIn(auth))
            }
            reqwest::StatusCode::ACCEPTED => Ok(DeviceFlowState::Pending),
            reqwest::StatusCode::GONE => Ok(DeviceFlowState::Expired),
            other => {
                let body = resp.text().await.unwrap_or_default();
                Err(Error::Server {
                    status: other.as_u16(),
                    body,
                })
            }
        }
    }

    /// Convenience: poll in a loop with the server-suggested interval until
    /// LoggedIn / Expired. Returns the AuthResponse on success.
    pub async fn await_login(&self, client: &CloudClient) -> Result<AuthResponse> {
        let interval = Duration::from_secs(self.interval_secs);
        loop {
            match self.poll(client).await? {
                DeviceFlowState::LoggedIn(auth) => return Ok(auth),
                DeviceFlowState::Pending => {
                    tokio::time::sleep(interval).await;
                }
                DeviceFlowState::Expired => return Err(Error::Other("device_code expired".into())),
            }
        }
    }
}
