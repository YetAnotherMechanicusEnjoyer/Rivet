use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tokio::sync::{Mutex, mpsc};
use tokio::time::{self, Duration};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage};

use crate::Error;
use crate::api::Presence;
use crate::api::guild::GuildMember;
use crate::logs::{LogType, print_log};
use crate::{AppAction, api::Message as DiscordMessage};

const GATEWAY_URL: &str = "wss://gateway.discord.gg/?v=10&encoding=json";

#[derive(Serialize)]
struct GatewayCommand {
    op: u8,
    d: serde_json::Value,
}

#[derive(Deserialize, Debug)]
struct GatewayEvent {
    op: u8,
    d: Option<serde_json::Value>,
    s: Option<u64>,
    t: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GatewayClient {
    token: String,
    action_tx: Sender<AppAction>,
    sequence: Arc<Mutex<Option<u64>>>,
    outbound_tx: Sender<serde_json::Value>,
    outbound_rx: Arc<Mutex<mpsc::Receiver<serde_json::Value>>>,
}

impl Default for GatewayClient {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel(32);
        Self {
            token: String::new(),
            action_tx: mpsc::channel::<AppAction>(32).0,
            sequence: Arc::new(Mutex::new(None)),
            outbound_tx: tx,
            outbound_rx: Arc::new(Mutex::new(rx)),
        }
    }
}

impl GatewayClient {
    pub fn new(token: String, action_tx: Sender<AppAction>) -> Self {
        let (tx, rx) = mpsc::channel(32);
        Self {
            token,
            action_tx,
            sequence: Arc::new(Mutex::new(None)),
            outbound_tx: tx,
            outbound_rx: Arc::new(Mutex::new(rx)),
        }
    }

    pub async fn connect(
        &self,
        mut rx_shutdown: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (ws_stream, _) = connect_async(GATEWAY_URL).await?;
        let (write, mut read) = ws_stream.split();

        let sequence = self.sequence.clone();
        let token = self.token.clone();
        let action_tx = self.action_tx.clone();

        // Wait for Hello to get heartbeat interval
        let heartbeat_interval = if let Some(Ok(msg)) = read.next().await {
            if let WsMessage::Text(text) = msg {
                let event: GatewayEvent = serde_json::from_str(&text)?;
                if event.op == 10 {
                    // Hello
                    let hello_data = event.d.unwrap();
                    hello_data["heartbeat_interval"].as_u64().unwrap_or(41250)
                } else {
                    return Err("Expected Hello".into());
                }
            } else {
                return Err("Expected Text Message".into());
            }
        } else {
            return Err("Connection Closed Before Hello".into());
        };

        let identify = serde_json::json!({
            "op": 2, // Identify
            "d": {
                "token": token,
                "capabilities": 30717,
                "properties": {
                    "os": "Windows",
                    "browser": "Chrome",
                    "device": "",
                    "system_locale": "en-US",
                    "browser_user_agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Safari/537.36",
                    "browser_version": "129.0.0.0",
                    "os_version": "10",
                    "referrer": "",
                    "referring_domain": "",
                    "referrer_current": "",
                    "referring_domain_current": "",
                    "release_channel": "stable",
                    "client_build_number": 332615,
                    "client_event_source": null,
                    "has_client_mods": false,
                    "client_launch_id": uuid::Uuid::new_v4().to_string(),
                    "client_heartbeat_session_id": uuid::Uuid::new_v4().to_string()
                },
                "presence": {
                    "status": "online",
                    "since": 0,
                    "activities": [],
                    "afk": false
                },
                "compress": false,
                "client_state": {
                    "guild_versions": {},
                    "highest_last_message_id": "0",
                    "read_state_version": 0,
                    "user_guild_settings_version": -1,
                    "user_settings_version": -1,
                    "private_channels_version": "0",
                    "api_code_version": 0
                }
            }
        });

        let write = Arc::new(Mutex::new(write));
        {
            let mut w = write.lock().await;
            w.send(WsMessage::Text(serde_json::to_string(&identify)?.into()))
                .await?;
        }

        // Start heartbeat task
        let write_clone = Arc::clone(&write);
        let seq_clone = Arc::clone(&sequence);
        let heartbeat_task = tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_millis(heartbeat_interval));
            loop {
                interval.tick().await;
                let seq = *seq_clone.lock().await;
                let op = GatewayCommand {
                    op: 1, // Heartbeat
                    d: serde_json::json!(seq),
                };
                let msg = WsMessage::Text(serde_json::to_string(&op).unwrap().into());
                let mut w = write_clone.lock().await;
                if let Err(e) = w.send(msg).await {
                    print_log(format!("Heartbeat failed: {}", e).into(), LogType::Error)
                        .await
                        .ok();
                    break;
                }
            }
        });

        let write_for_outbound = Arc::clone(&write);
        let mut rx_outbound = self.outbound_rx.lock().await;

        // Listen for events
        loop {
            tokio::select! {
                _ = rx_shutdown.recv() => {
                    break;
                }
                Some(outbound_msg) = rx_outbound.recv() => {
                    let mut w = write_for_outbound.lock().await;
                    if let Err(e) = w.send(WsMessage::Text(serde_json::to_string(&outbound_msg).unwrap().into())).await {
                        print_log(format!("Failed to send outbound message: {}", e).into(), LogType::Error).await.ok();
                    }
                }
                msg_result = read.next() => {
                    match msg_result {
                        Some(Ok(WsMessage::Text(text))) => {
                            if let Ok(event) = serde_json::from_str::<GatewayEvent>(&text) {
                                if let Some(s) = event.s {
                                    let mut seq = sequence.lock().await;
                                    *seq = Some(s);
                                }

                                if event.op == 0 {
                                    // Dispatch
                                    if let (Some(t), Some(d)) = (event.t, event.d) {
                                        Self::handle_dispatch(&t, d, &action_tx).await;
                                    }
                                }
                            }
                        }
                        Some(Ok(WsMessage::Close(_))) => {
                            break;
                        }
                        Some(Err(e)) => {
                            print_log(format!("Gateway error: {}", e).into(), LogType::Error).await.ok();
                            break;
                        }
                        None => {
                            print_log("Gateway connection closed unexpectedly".into(), LogType::Error).await.ok();
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }

        heartbeat_task.abort();
        Ok(())
    }

    async fn handle_dispatch(t: &str, d: serde_json::Value, action_tx: &Sender<AppAction>) {
        match t {
            "MESSAGE_CREATE" => match serde_json::from_value::<DiscordMessage>(d) {
                Ok(msg) => {
                    action_tx
                        .send(AppAction::GatewayMessageCreate(msg))
                        .await
                        .ok();
                }
                Err(e) => {
                    print_log(
                        format!("Failed to parse created message: {e}").into(),
                        LogType::Error,
                    )
                    .await
                    .ok();
                }
            },
            "MESSAGE_UPDATE" => match serde_json::from_value::<crate::api::PartialMessage>(d) {
                Ok(msg) => {
                    action_tx
                        .send(AppAction::GatewayMessageUpdate(msg))
                        .await
                        .ok();
                }
                Err(e) => {
                    print_log(
                        format!("Failed to parse updated message: {e}").into(),
                        LogType::Error,
                    )
                    .await
                    .ok();
                }
            },
            "MESSAGE_DELETE" => match (d["id"].as_str(), d["channel_id"].as_str()) {
                (Some(id), Some(channel_id)) => {
                    action_tx
                        .send(AppAction::GatewayMessageDelete(
                            id.to_string(),
                            channel_id.to_string(),
                        ))
                        .await
                        .ok();
                }
                _ => {
                    print_log("Failed to parse deleted message.".into(), LogType::Error)
                        .await
                        .ok();
                }
            },
            "TYPING_START" => {
                if let (Some(channel_id), Some(user_id), Some(_timestamp)) = (
                    d["channel_id"].as_str(),
                    d["user_id"].as_str(),
                    d["timestamp"].as_u64(),
                ) {
                    // Prefer guild nick, fall back to member username, then top-level user (DMs)
                    let display_name = d["member"]["nick"]
                        .as_str()
                        .or_else(|| d["member"]["user"]["username"].as_str())
                        .or_else(|| d["user"]["username"].as_str())
                        .map(|s| s.to_string());
                    let _ = action_tx
                        .send(AppAction::GatewayTypingStart(
                            channel_id.to_string(),
                            user_id.to_string(),
                            display_name,
                        ))
                        .await;
                }
            }
            "READY_SUPPLEMENTAL" => {
                let mut statuses = std::collections::HashMap::new();
                let mut status_texts = std::collections::HashMap::new();
                if let Some(guilds) = d["merged_presences"]["guilds"].as_array() {
                    for guild in guilds {
                        if let Some(users) = guild.as_array() {
                            for user in users {
                                if let (Some(user_id), Some(status)) =
                                    (user["user_id"].as_str(), user["status"].as_str())
                                {
                                    statuses.insert(user_id.to_string(), status.to_string());
                                }
                                // Extract custom status text from activities (type 4)
                                if let (Some(user_id), Some(activities)) =
                                    (user["user_id"].as_str(), user["activities"].as_array())
                                {
                                    for activity in activities {
                                        if activity["type"].as_u64() == Some(4)
                                            && let Some(state_text) = activity["state"].as_str()
                                        {
                                            status_texts.insert(
                                                user_id.to_string(),
                                                state_text.to_string(),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if let Some(friends) = d["merged_presences"]["friends"].as_array() {
                    for friend in friends {
                        if let (Some(user_id), Some(status)) =
                            (friend["user_id"].as_str(), friend["status"].as_str())
                        {
                            statuses.insert(user_id.to_string(), status.to_string());
                        }
                        // Extract custom status text from friend activities (type 4)
                        if let (Some(user_id), Some(activities)) =
                            (friend["user_id"].as_str(), friend["activities"].as_array())
                        {
                            for activity in activities {
                                if activity["type"].as_u64() == Some(4)
                                    && let Some(state_text) = activity["state"].as_str()
                                {
                                    status_texts
                                        .insert(user_id.to_string(), state_text.to_string());
                                }
                            }
                        }
                    }
                }
                let _ = action_tx
                    .send(AppAction::GatewayReadySupplemental(statuses, status_texts))
                    .await;
            }
            "PRESENCE_UPDATE" => match serde_json::from_value::<Presence>(d.clone()) {
                Ok(presence) => {
                    action_tx
                        .send(AppAction::GatewayPresenceUpdate(presence))
                        .await
                        .ok();
                }
                Err(e) => {
                    print_log(
                        format!("Failed to parse presence: {e}").into(),
                        LogType::Error,
                    )
                    .await
                    .ok();
                }
            },
            "GUILD_MEMBERS_CHUNK" => {
                if let Ok(not_found) = serde_json::from_value::<Vec<String>>(d["not_found"].clone())
                {
                    print_log(
                        format!("Error in GUILD_MEMBERS_CHUNK event: not found: {not_found:?}")
                            .into(),
                        LogType::Error,
                    )
                    .await
                    .ok();
                }
                if let (Some(guild_id), Ok(members), Some(chunk_index), Some(chunk_count)) = (
                    d["guild_id"].as_str(),
                    serde_json::from_value::<Vec<GuildMember>>(d["members"].clone()),
                    d["chunk_index"].as_number(),
                    d["chunk_count"].as_number(),
                ) {
                    action_tx
                        .send(AppAction::GatewayGuildMembersChunk(
                            guild_id.to_string(),
                            members.clone(),
                            chunk_index.to_string(),
                            chunk_count.to_string(),
                        ))
                        .await
                        .ok();
                }
            }
            "SESSIONS_REPLACE" => {
                print_log(format!("SESSION_REPLACE:{d}").into(), LogType::Info)
                    .await
                    .ok();
            }
            "USER_SETTINGS_PROTO_UPDATE" | "READY" => {}
            e => {
                print_log(
                    format!("Unhandled event received: {e}").into(),
                    LogType::Info,
                )
                .await
                .ok();
            }
        }
    }

    pub async fn request_guild_members(&self, guild_id: &str) -> Result<(), Error> {
        let request = serde_json::json!({
            "op": 8,
            "d": {
                "guild_id": guild_id,
                "query": "",
                "limit": 0,
            }
        });

        if let Err(e) = self.outbound_tx.send(request).await {
            print_log(
                format!("Error sending request guild members: {e}").into(),
                LogType::Error,
            )
            .await
            .ok();
        }

        Ok(())
    }

    pub async fn subscribe_channel(&self, guild_id: &str, channel_id: &str) -> Result<(), Error> {
        let request = serde_json::json!({
            "op": 14,
            "d": {
                "guild_id": guild_id,
                "channels": {
                    channel_id: [[0, 99]]
                },
                "typing": true,
                "threads": true,
                "activities": true,
            }
        });

        if let Err(e) = self.outbound_tx.send(request).await {
            print_log(
                format!("Error subscribing to a channel: {e}").into(),
                LogType::Error,
            )
            .await
            .ok();
        }

        Ok(())
    }

    pub async fn update_presence(
        &self,
        status: &str,
        custom_status: Option<&str>,
    ) -> Result<(), Error> {
        let mut activities = vec![];
        if let Some(text) = custom_status {
            activities.push(serde_json::json!({
                "type": 4,
                "name": "Custom Status",
                "state": text
            }));
        }

        let request = serde_json::json!({
            "op": 3,
            "d": {
                "status": status,
                "since": 0,
                "activities": activities,
                "afk": false,
            }
        });

        if let Err(e) = self.outbound_tx.send(request).await {
            print_log(
                format!("Error updating presence via gateway: {e}").into(),
                LogType::Error,
            )
            .await
            .ok();
        }

        Ok(())
    }
}
