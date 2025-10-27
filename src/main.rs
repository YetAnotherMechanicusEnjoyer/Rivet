use std::{env, process};

use crate::{api::message::get_channel_messages::get_channel_messages, model::channel::Message};

pub mod api;
pub mod model;

pub type Error = Box<dyn std::error::Error>;

#[tokio::main]
async fn main() -> Result<(), Error> {
    dotenvy::dotenv().ok();
    const ENV_TOKEN: &str = "DISCORD_TOKEN";
    const ENV_CHANNEL: &str = "DISCORD_CHANNEL";

    let args: Vec<String> = env::args().collect();
    let limit = args.get(1);

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
        get_channel_messages(channel_id, token, None, None, None, limit.cloned()).await?;

    messages.iter().for_each(|msg| {
        let content = msg
            .content
            .clone()
            .unwrap_or_else(|| "(*Non-text message*)".to_string());
        println!("{} -> {content}", msg.author.username);
    });

    messages.iter().for_each(|msg| println!("{msg:?}"));

    Ok(())
}
