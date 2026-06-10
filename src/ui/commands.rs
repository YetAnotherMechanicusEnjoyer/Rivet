use tokio::sync::{MutexGuard, mpsc::Sender};

use crate::{
    App, AppAction, KeywordAction,
    logs::{LogType, print_log},
};

pub async fn handle_command(
    state: &mut MutexGuard<'_, App>,
    tx_action: &Sender<AppAction>,
    input: String,
) -> Option<KeywordAction> {
    let (cmd, args) = {
        let mut s = input.split(' ');
        (
            s.next().unwrap_or_default().to_lowercase(),
            s.map(|s| s.to_string()).collect::<Vec<String>>(),
        )
    };

    match cmd.as_str() {
        "quit" | "q" => {
            return Some(KeywordAction::Break);
        }
        "debug" => {
            print_log(args.join(" ").as_str().into(), LogType::Debug)
                .await
                .ok();
        }
        "logs" => {
            tx_action.send(AppAction::TransitionToLogs).await.ok();
        }
        "status" => {
            if let Some(status) = args.first() {
                let status_text = if args.len() > 1 {
                    Some(args[1..].join(" "))
                } else {
                    None
                };

                let actual_status = if status == "invisible_dnd" {
                    state.data.notifs.is_invisible_dnd = true;
                    "invisible"
                } else {
                    state.data.notifs.is_invisible_dnd = false;
                    status.as_str()
                };

                let mut settings_payload = serde_json::json!({
                    "status": actual_status,
                });

                if let Some(text) = &status_text {
                    settings_payload["custom_status"] = serde_json::json!({
                        "text": text,
                    });
                } else {
                    settings_payload["custom_status"] = serde_json::json!(null);
                }

                if let Err(e) = state
                    .client
                    .api
                    .modify_user_settings(settings_payload)
                    .await
                {
                    print_log(
                        format!("Failed to change status settings: {e}").into(),
                        LogType::Error,
                    )
                    .await
                    .ok();
                }

                if let Err(e) = state
                    .client
                    .gateway
                    .update_presence(actual_status, status_text.as_deref())
                    .await
                {
                    print_log(
                        format!("Failed to update live presence: {e}").into(),
                        LogType::Error,
                    )
                    .await
                    .ok();
                } else {
                    if let Some(user) = state.data.current_user.clone() {
                        state
                            .data
                            .dms
                            .user_statuses
                            .insert(user.id.clone(), actual_status.to_string());
                        if let Some(text) = &status_text {
                            state
                                .data
                                .dms
                                .user_status_texts
                                .insert(user.id.clone(), text.clone());
                        } else {
                            state.data.dms.user_status_texts.remove(&user.id);
                        }
                    }
                }
            } else {
                print_log(
                    "Failed to change status: Bad usage: \"status <online|dnd|idle|invisible|invisible_dnd> [text]\"".into(),
                    LogType::Error,
                ).await.ok();
            }
        }
        _ => {}
    }
    None
}
