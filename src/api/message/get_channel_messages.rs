use reqwest::Client;

use crate::Message;

pub async fn get_channel_messages(
    channel_id: &str,
    token: &str,
    around: Option<String>,
    before: Option<String>,
    after: Option<String>,
    limit: Option<String>,
) -> Result<Vec<Message>, Box<dyn std::error::Error>> {
    let mut api_url = format!("https://discord.com/api/v10/channels/{channel_id}/messages?");

    if let Some(around) = around {
        api_url.push_str(format!("around={}&", around.as_str()).as_str());
    }
    if let Some(before) = before {
        api_url.push_str(format!("before={}&", before.as_str()).as_str());
    }
    if let Some(after) = after {
        api_url.push_str(format!("after={}&", after.as_str()).as_str());
    }
    if let Some(limit) = limit {
        api_url.push_str(format!("limit={}", limit.as_str()).as_str());
    }

    let client = Client::new();

    let response = client
        .get(&api_url)
        .header("Authorization", token)
        .send()
        .await?;

    if !response.status().is_success() {
        eprintln!("API Error: Status code {}", response.status());
        eprintln!("Body response: {}", response.text().await?);
        return Err("Failed to request Discord API".into());
    }

    let mut messages: Vec<Message> = response.json().await?;

    messages.reverse();

    Ok(messages)
}
