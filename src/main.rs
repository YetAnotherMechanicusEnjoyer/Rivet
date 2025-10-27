use serde::Deserialize;
use std::{env, process};

use crate::api::message::get_channel_messages::get_channel_messages;

pub mod api;

#[derive(Debug, Deserialize)]
struct Author {
    username: String,
}

#[derive(Debug, Deserialize)]
pub struct Message {
    content: Option<String>,
    author: Author,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    let default_limit = env::var("DEFAULT_LIMIT").unwrap_or("50".into());
    const ENV_TOKEN: &str = "DISCORD_TOKEN";
    const ENV_CHANNEL: &str = "DISCORD_CHANNEL";

    let args: Vec<String> = env::args().collect();
    let limit = args
        .get(1)
        .map(|s| s.as_str())
        .unwrap_or(default_limit.as_str());

    let channel_id = args.get(2).map(|s| s.as_str()).unwrap_or_else(|| {
        env::var(ENV_CHANNEL)
            .unwrap_or_else(|_| {
                eprintln!("Error: DISCORD_CHANNEL variable is missing.");
                process::exit(1);
            })
            .to_string()
            .leak()
    });

    let token: &str = &env::var(ENV_TOKEN).unwrap_or_else(|_| {
        eprintln!("Error: DISCORD_TOKEN variable is missing.");
        process::exit(1);
    });

    let messages: Vec<Message> =
        get_channel_messages(channel_id, token, None, None, None, Some(limit.to_string())).await?;

    messages.iter().for_each(|msg| {
        let content = msg
            .content
            .clone()
            .unwrap_or_else(|| "(*Non-text message*)".to_string());
        println!("{} -> {content}", msg.author.username);
    });

    Ok(())
}
