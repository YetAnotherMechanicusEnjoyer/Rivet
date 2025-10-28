use serde::Deserialize;

pub type Snowflake = u64;

pub type Timestamp = String;

#[derive(Debug, Deserialize)]
pub struct User {
    //pub id: Snowflake,
    pub username: String,
    //pub discriminator: String,
    //pub global_name: Option<String>,
    //pub avatar : Option<String>,
    //pub bot: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct Role {}

#[derive(Debug, Deserialize)]
pub struct ChannelMention {}

#[derive(Debug, Deserialize)]
pub struct Attachment {}

#[derive(Debug, Deserialize)]
pub struct Embed {}

#[derive(Debug, Deserialize)]
pub struct Reaction {}

#[derive(Debug, Deserialize)]
pub struct MessageActivity {}

#[derive(Debug, Deserialize)]
pub struct Application {}

#[derive(Debug, Deserialize)]
pub struct MessageReference {}

#[derive(Debug, Deserialize)]
pub struct MessageSnapshot {}

#[derive(Debug, Deserialize)]
pub struct MessageInteraction {}

#[derive(Debug, Deserialize)]
pub struct Channel {}

#[derive(Debug, Deserialize)]
pub struct MessageInteractionMetadata {}

#[derive(Debug, Deserialize)]
pub struct MessageComponent {}

#[derive(Debug, Deserialize)]
pub struct MessageStickerItem {}

#[derive(Debug, Deserialize)]
pub struct Sticker {}

#[derive(Debug, Deserialize)]
pub struct RoleSubscriptionData {}

#[derive(Debug, Deserialize)]
pub struct Resolved {}

#[derive(Debug, Deserialize)]
pub struct Poll {}

#[derive(Debug, Deserialize)]
pub struct MessageCall {}

#[derive(Debug, Deserialize)]
pub enum Nonce {
    String(String),
    Integer(i64),
}

#[derive(Debug, Deserialize)]
pub struct Message {
    //pub id: Snowflake,
    //pub channel_id: Snowflake,
    pub author: User,
    pub content: Option<String>,
    /*pub timestamp: Timestamp,
    pub edited_timestamp: Option<Timestamp>,
    pub tts: bool,
    pub mention_everyone: bool,
    pub mentions: Vec<User>,
    pub mention_roles: Vec<Role>,
    pub mention_channels: Vec<ChannelMention>,
    pub attachments: Vec<Attachment>,
    pub embeds: Vec<Embed>,
    pub reactions: Vec<Reaction>,
    pub nonce: Nonce,
    pub pinned: bool,
    pub webhook_id: Option<Snowflake>,
    pub message_type: i32,
    pub activity: Option<MessageActivity>,
    pub application: Option<Application>,
    pub application_id: Snowflake,
    pub flags: Option<i32>,
    pub message_reference: Option<MessageReference>,
    pub message_snapshots: Option<Vec<MessageSnapshot>>,
    pub referenced_message: Option<Box<Message>>,
    pub interaction_metadata: Option<Box<MessageInteractionMetadata>>,
    pub interaction: Option<Box<MessageInteraction>>,
    pub thread: Option<Channel>,
    pub components: Option<Vec<MessageComponent>>,
    pub sticker_items: Option<Vec<MessageStickerItem>>,
    pub stickers: Option<Vec<Sticker>>,
    pub position: i32,
    pub role_subscription_data: Option<RoleSubscriptionData>,
    pub resolved: Option<Resolved>,
    pub poll: Option<Box<Poll>>,
    pub call: Option<MessageCall>,*/
}
