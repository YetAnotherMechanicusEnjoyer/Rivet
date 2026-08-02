use crate::{
    AppAction, Client, DISCORD_BASE_URL, Error,
    api::{ApiClient, GatewayClient},
};
use keyring::KeyringEntry;
use reqwest::Client as ReqwestClient;
use tokio::sync::mpsc::Sender;

#[derive(Debug)]
pub struct Auth {
    pub token_entry: KeyringEntry,
}

impl Auth {
    pub async fn store_token(&self, token: &str) -> Result<(), Error> {
        self.token_entry
            .set_secret(token)
            .await
            .map_err(|e| e.into())
    }

    pub async fn validate_token(
        &self,
        token: &str,
        tx_action: Sender<AppAction>,
    ) -> Result<Client, Error> {
        let client = Client::new(
            ApiClient::new(
                ReqwestClient::new(),
                token.to_string(),
                DISCORD_BASE_URL.to_string(),
            ),
            GatewayClient::new(token.to_string(), tx_action.clone()),
        );

        client
            .api
            .get_current_user()
            .await
            .map(|_| client)
            .map_err(|e| {
                let err_msg = e.to_string();
                if err_msg.contains("401") {
                    "Please check your session token and try again.".into()
                } else if err_msg.contains("API GET request") {
                    err_msg.into()
                } else {
                    "Could not connect to the server. Please check your internet connection.".into()
                }
            })
    }
}

impl Default for Auth {
    fn default() -> Self {
        let token_entry =
            KeyringEntry::try_new(env!("CARGO_PKG_NAME")).expect("Error creating keyring entry.");
        Self { token_entry }
    }
}
