mod blocks;
mod files;
mod http;
mod socket;
mod types;

pub use blocks::attachments_to_cards;
#[allow(unused_imports)]
pub use types::{
    BotProfile, SlackAttachment, SlackFile, SlackMessage, SlackReaction, SlackUpdate,
};

use anyhow::{anyhow, Result};
use reqwest::Client as HttpClient;
use std::sync::Arc;
use tokio::sync::Mutex;

use types::AuthTestResponse;

#[derive(Clone)]
pub struct SlackClient {
    pub(crate) http: HttpClient,
    pub(crate) token: String,
    pub(crate) user_id: Arc<Mutex<Option<String>>>,
    pub(crate) pending_updates: Arc<Mutex<Vec<types::SlackUpdate>>>,
    pub(crate) ws_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    pub(crate) ws_shutdown: Arc<Mutex<Option<tokio::sync::broadcast::Sender<()>>>>,
    pub(crate) user_name_cache: Arc<Mutex<std::collections::HashMap<String, String>>>,
    pub(crate) user_info_cache: Arc<Mutex<std::collections::HashMap<String, types::CachedUserInfo>>>,
}

impl SlackClient {
    pub async fn new(token: &str, _app_token: &str) -> Result<Self> {
        let http = HttpClient::new();
        let token = token.to_string();

        let client = Self {
            http,
            token,
            user_id: Arc::new(Mutex::new(None)),
            pending_updates: Arc::new(Mutex::new(Vec::new())),
            ws_handle: Arc::new(Mutex::new(None)),
            ws_shutdown: Arc::new(Mutex::new(None)),
            user_name_cache: Arc::new(Mutex::new(std::collections::HashMap::new())),
            user_info_cache: Arc::new(Mutex::new(std::collections::HashMap::new())),
        };

        // Test authentication
        let auth_response: AuthTestResponse = client
            .http
            .get("https://slack.com/api/auth.test")
            .bearer_auth(&client.token)
            .send()
            .await?
            .json()
            .await?;

        if !auth_response.ok {
            return Err(anyhow!("Slack authentication failed"));
        }

        *client.user_id.lock().await = Some(auth_response.user_id);

        Ok(client)
    }

    pub async fn get_my_user_id(&self) -> Result<String> {
        let user_id = self.user_id.lock().await;
        user_id.clone().ok_or_else(|| anyhow!("User ID not set"))
    }
    pub async fn get_pending_updates(&self) -> Vec<types::SlackUpdate> {
        let mut updates = self.pending_updates.lock().await;
        std::mem::take(&mut *updates)
    }
    pub async fn shutdown(&self) {
        tracing::debug!("shutdown() called");
        // Send shutdown signal to gracefully close WebSocket
        if let Some(tx) = self.ws_shutdown.lock().await.take() {
            let _ = tx.send(());
            tracing::debug!("Shutdown signal sent");
        }
        
        // Wait for the task to finish (with timeout)
        if let Some(handle) = self.ws_handle.lock().await.take() {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), handle).await;
            tracing::debug!("WebSocket task finished");
        }
    }
}
