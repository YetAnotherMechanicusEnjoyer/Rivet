pub mod activity;
pub mod channel;
pub mod dm;
pub mod emoji;
pub mod gateway;
pub mod guild;
pub mod message;
pub mod presence;
pub mod user;

use reqwest::{Client, Method};
use serde::de::DeserializeOwned;

pub use activity::Activity;
pub use channel::Channel;
pub use dm::DM;
pub use emoji::Emoji;
pub use gateway::GatewayClient;
pub use guild::Guild;
pub use message::Message;
pub use message::PartialMessage;
pub use presence::Presence;
pub use user::User;

use crate::api::guild::PartialGuild;
use crate::api::user::UserSettings;
use crate::{
    Error,
    api::{
        channel::{PermissionContext, Role},
        guild::GuildMember,
    },
};

#[derive(Debug, Clone)]
pub enum AnyChannel {
    Guild(Channel),
    Direct(DM),
}

impl AnyChannel {
    pub fn get_id(&self) -> String {
        match self {
            Self::Guild(ch) => ch.id.clone(),
            Self::Direct(dm) => dm.id.clone(),
        }
    }
    pub fn get_channel_type(&self) -> u8 {
        match self {
            Self::Guild(ch) => ch.channel_type,
            Self::Direct(dm) => dm.channel_type,
        }
    }
    pub fn get_name(&self) -> String {
        match self {
            Self::Guild(ch) => ch.name.clone(),
            Self::Direct(dm) => dm.get_name(),
        }
    }
    pub fn get_guild_id(&self) -> Option<String> {
        match self {
            Self::Guild(ch) => ch.guild_id.clone(),
            Self::Direct(_) => None,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct ApiClient {
    pub http_client: Client,
    pub auth_token: String,
    pub base_url: String,
}

impl ApiClient {
    pub fn new(http_client: Client, auth_token: String, base_url: String) -> Self {
        Self {
            http_client,
            auth_token,
            base_url,
        }
    }

    async fn api_request<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        method: Method,
        body: Option<serde_json::Value>,
    ) -> Result<T, Error> {
        let url = format!("{}/{}", self.base_url, endpoint);
        let mut request = self
            .http_client
            .request(method, &url)
            .header("Authorization", self.auth_token.as_str());

        if let Some(data) = body {
            request = request.json(&data);
        }

        let response = request.send().await?;
        let status = response.status();

        if status.is_success() {
            Ok(response.json::<T>().await?)
        } else {
            let body = response
                .text()
                .await
                .unwrap_or("Failed to read error body".to_string());
            Err(format!("API Error: Status {status}. Details: {body}").into())
        }
    }

    async fn api_request_no_content(
        &self,
        endpoint: &str,
        method: Method,
        body: Option<serde_json::Value>,
    ) -> Result<(), Error> {
        let url = format!("{}/{}", self.base_url, endpoint);
        let mut request = self
            .http_client
            .request(method, &url)
            .header("Authorization", self.auth_token.as_str());

        if let Some(data) = body {
            request = request.json(&data);
        }

        let response = request.send().await?;
        let status = response.status();

        if status.is_success() {
            Ok(())
        } else {
            let body = response
                .text()
                .await
                .unwrap_or("Failed to read error body".to_string());
            Err(format!("API Error: Status {status}. Details: {body}").into())
        }
    }

    pub async fn get_channel(&self, channel_id: &str) -> Result<AnyChannel, Error> {
        let json: serde_json::Value = self
            .api_request(format!("channels/{channel_id}").as_str(), Method::GET, None)
            .await?;

        let channel_type = json.get("type").and_then(|t| t.as_u64()).unwrap_or(0) as u8;

        match channel_type {
            1 | 3 => {
                let dm: DM = serde_json::from_value(json)?;
                Ok(AnyChannel::Direct(dm))
            }
            _ => {
                let channel: Channel = serde_json::from_value(json)?;
                Ok(AnyChannel::Guild(channel))
            }
        }
    }

    pub async fn get_current_user(&self) -> Result<User, Error> {
        self.api_request("users/@me", Method::GET, None).await
    }

    pub async fn get_dms(&self) -> Result<Vec<DM>, Error> {
        self.api_request("users/@me/channels", Method::GET, None)
            .await
    }

    pub async fn get_guild(&self, guild_id: &str) -> Result<Guild, Error> {
        self.api_request(format!("guilds/{guild_id}").as_str(), Method::GET, None)
            .await
    }

    pub async fn get_guild_emojis(&self, guild_id: &str) -> Result<Vec<Emoji>, Error> {
        self.api_request(
            format!("guilds/{guild_id}/emojis").as_str(),
            Method::GET,
            None,
        )
        .await
    }

    pub async fn get_guild_channels(&self, guild_id: &str) -> Result<Vec<Channel>, Error> {
        self.api_request(
            format!("guilds/{guild_id}/channels").as_str(),
            Method::GET,
            None,
        )
        .await
    }

    pub async fn get_guild_roles(&self, guild_id: &str) -> Result<Vec<Role>, Error> {
        self.api_request(
            format!("guilds/{guild_id}/roles").as_str(),
            Method::GET,
            None,
        )
        .await
    }

    pub async fn get_guild_member(&self, guild_id: &str) -> Result<GuildMember, Error> {
        let user = self.get_current_user().await?;
        self.api_request(
            format!("guilds/{guild_id}/members/{}", user.id).as_str(),
            Method::GET,
            None,
        )
        .await
    }

    pub async fn get_permission_context(&self, guild_id: &str) -> Result<PermissionContext, Error> {
        let all_guild_roles: Vec<Role> = self.get_guild_roles(guild_id).await?;
        let member_info: GuildMember = self.get_guild_member(guild_id).await?;

        Ok(PermissionContext {
            user_id: member_info.user.id,
            user_role_ids: {
                let everyone_role_id = guild_id.to_string();
                let mut roles = member_info.roles;
                if !roles.contains(&everyone_role_id) {
                    roles.push(everyone_role_id.clone());
                }
                roles
            },
            all_guild_roles,
            everyone_role_id: guild_id.to_string(),
        })
    }

    pub async fn create_message(
        &self,
        channel_id: &str,
        content: Option<String>,
        tts: bool,
    ) -> Result<Message, Error> {
        self.api_request(
            format!("channels/{channel_id}/messages").as_str(),
            Method::POST,
            Some(serde_json::json!({ "content": content, "tts": tts })),
        )
        .await
    }

    pub async fn edit_message(
        &self,
        channel_id: &str,
        message_id: &str,
        content: Option<String>,
    ) -> Result<Message, Error> {
        self.api_request(
            format!("channels/{channel_id}/messages/{message_id}").as_str(),
            Method::PATCH,
            Some(serde_json::json!({ "content": content})),
        )
        .await
    }

    pub async fn delete_message(&self, channel_id: &str, message_id: &str) -> Result<(), Error> {
        self.api_request_no_content(
            format!("channels/{channel_id}/messages/{message_id}").as_str(),
            Method::DELETE,
            None,
        )
        .await
    }

    pub async fn get_channel_messages(
        &self,
        channel_id: &str,
        around: Option<String>,
        before: Option<String>,
        after: Option<String>,
        limit: Option<usize>,
    ) -> Result<Vec<Message>, Error> {
        let mut endpoint = format!("channels/{channel_id}/messages");
        let mut query = Vec::new();

        if let Some(a) = around {
            query.push(format!("around={a}"));
        }
        if let Some(b) = before {
            query.push(format!("before={b}"));
        }
        if let Some(a) = after {
            query.push(format!("after={a}"));
        }
        if let Some(l) = limit {
            query.push(format!("limit={l}"));
        }

        if !query.is_empty() {
            endpoint.push('?');
            endpoint.push_str(&query.join("&"));
        }

        self.api_request(&endpoint, Method::GET, None).await
    }

    pub async fn trigger_typing_indicator(&self, channel_id: &str) -> Result<(), Error> {
        self.api_request_no_content(
            format!("channels/{channel_id}/typing").as_str(),
            Method::POST,
            None,
        )
        .await
    }

    pub async fn ack_message(&self, channel_id: &str, message_id: &str) -> Result<(), Error> {
        self.api_request_no_content(
            format!("channels/{channel_id}/messages/{message_id}/ack").as_str(),
            Method::POST,
            Some(serde_json::json!({"token": null})),
        )
        .await
    }

    pub async fn get_current_user_guilds(&self) -> Result<Vec<PartialGuild>, Error> {
        self.api_request("/users/@me/guilds", Method::GET, None)
            .await
    }

    pub async fn modify_user_settings(
        &self,
        value: serde_json::Value,
    ) -> Result<UserSettings, Error> {
        self.api_request("/users/@me/settings", Method::PATCH, Some(value))
            .await
    }
}
