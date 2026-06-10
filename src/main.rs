use std::{
    collections::{HashMap, HashSet},
    env, io, process,
    sync::Arc,
    time::Duration,
};

use crossterm::{
    cursor::SetCursorStyle,
    event::EnableBracketedPaste,
    execute,
    terminal::{EnterAlternateScreen, enable_raw_mode},
};
use ratatui::{Terminal, prelude::CrosstermBackend};
use reqwest::Client as ReqwestClient;
use tokio::{
    sync::{Mutex, mpsc},
    task::JoinHandle,
    time,
};

use crate::{
    api::{
        AnyChannel, ApiClient, Channel, Emoji, GatewayClient, Guild, Message, PartialMessage,
        Presence, User,
        channel::PermissionContext,
        dm::DM,
        guild::{GuildMember, PartialGuild},
    },
    logs::{LogReader, LogType, get_log_directory, print_log, watch_logs},
    signals::{restore_terminal, setup_ctrlc_handler},
    ui::{draw_ui, handle_input_events, handle_keys_events, vim::VimState},
};

mod api;
mod config;
mod logs;
mod signals;
mod ui;

const APP_NAME: &str = "vimcord";
const VIM_FLAG: &str = "--vim";
const LOGS_FOLDER: &str = "logs";

const DISCORD_BASE_URL: &str = "https://discord.com/api/v10";

pub type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

#[derive(Debug)]
pub enum KeywordAction {
    Continue,
    Break,
}

#[derive(Debug, Default, Clone)]
pub enum Window {
    #[default]
    Home,
    Guild,
    DM,
    Channel(Box<Guild>),
    Chat(Box<AnyChannel>),
}

#[derive(Debug, Default, Clone)]
pub enum AppState {
    #[default]
    Home,
    SelectingGuild,
    SelectingDM,
    SelectingChannel(Box<Guild>),
    Chatting(Box<AnyChannel>),
    EmojiSelection(Box<AnyChannel>),
    Editing(Box<AnyChannel>, Box<Message>, String),
    Loading(Window),
    Logs(Window),
}

#[derive(Debug)]
pub enum AppAction {
    SigInt,
    InputChar(char),
    InputBackspace,
    InputDelete,
    InputEscape,
    InputSubmit,
    SelectNext,
    SelectPrevious,
    SelectLeft,
    SelectRight,
    ApiDeleteMessage(String, String),
    ApiEditMessage(String, String, String),
    ApiUpdateMessages(String, Vec<Message>),
    ApiUpdateChannel(Vec<Channel>),
    ApiUpdateEmojis(Vec<Emoji>),
    ApiUpdateGuilds(Vec<PartialGuild>),
    ApiUpdateDMs(Vec<DM>),
    ApiUpdateContext(Option<PermissionContext>),
    ApiUpdateCurrentUser(User),
    GatewayMessageCreate(Message),
    GatewayMessageUpdate(PartialMessage),
    GatewayMessageDelete(String, String), // message_id, channel_id
    GatewayTypingStart(String, String, Option<String>), // channel_id, user_id, display_name
    GatewayGuildMembersChunk(String, Vec<GuildMember>, String, String),
    GatewayReadySupplemental(
        std::collections::HashMap<String, String>,
        std::collections::HashMap<String, String>,
    ), // (user_id -> status, user_id -> status_text)
    GatewayPresenceUpdate(Presence), // user_id, status, custom_status_text
    TransitionToChat(Box<AnyChannel>),
    TransitionToEditing(Box<AnyChannel>, Message, String, char),
    TransitionToChannels(Box<Guild>),
    TransitionToGuilds,
    TransitionToDM,
    TransitionToHome,
    TransitionToLoading(Window),
    TransitionToLogs,
    TransitionToLoadingMessages,
    EndLoading,
    EndLogs,
    EndLoadingMessages,
    SelectEmoji,
    Paste(String),
    DesktopNotification(String, String, String),
    Tick,
    NewLogReceived(String),
    ClearLogs,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub enum InputMode {
    #[default]
    Normal,
    Insert,
    Command,
    Search,
    Visual,
    VisualLine,
}

#[derive(Default, Debug)]
struct Client {
    api: ApiClient,
    gateway: GatewayClient,
}

impl Client {
    pub fn new(api: ApiClient, gateway: GatewayClient) -> Self {
        Self { api, gateway }
    }
}

#[derive(Default, Debug)]
struct GuildData {
    selected: Option<Guild>,
    members: Vec<GuildMember>,
    channels: Vec<Channel>,
    messages: Vec<Message>,
    custom_emojis: Vec<Emoji>,

    joined: Vec<PartialGuild>,
}

#[derive(Default, Debug)]
struct DMData {
    channels: Vec<DM>,
    user_names: HashMap<String, String>,
    user_statuses: HashMap<String, String>, // user id -> status string (online, offline, etc.)
    user_status_texts: HashMap<String, String>, // user id -> custom status text
}

#[derive(Default, Debug)]
struct EmojiData {
    map: Vec<(String, String)>,
    filter: String,
    index: usize,
    /// Byte position where the emoji filter started (position of the ':')
    filter_start: Option<usize>,
}

impl EmojiData {
    pub fn new(map: Vec<(String, String)>) -> Self {
        Self {
            map,
            ..Default::default()
        }
    }
}

#[derive(Default, Debug)]
struct InputData {
    buffer: String,
    saved: Option<String>,
    search: String,
    mode: InputMode,
}

#[derive(Default, Debug)]
struct VimData {
    active: bool,
    state: Option<VimState>,
}

impl VimData {
    pub fn new(vim_mode: bool) -> Self {
        let vim_mode = vim_mode || env::args().any(|arg| arg == VIM_FLAG);
        let vim_state = if vim_mode {
            Some(VimState::default())
        } else {
            None
        };

        Self {
            active: vim_mode,
            state: vim_state,
        }
    }
}

#[derive(Default, Debug)]
struct NotifsData {
    pub last_message_ids: HashMap<String, String>,
    pub discreet: bool,
    #[cfg(not(target_os = "windows"))]
    pub active: HashMap<String, Vec<notify_rust::NotificationHandle>>,
    pub is_invisible_dnd: bool,
}

impl NotifsData {
    pub fn new(discreet: bool) -> Self {
        Self {
            discreet,
            ..Default::default()
        }
    }
}

#[derive(Default, Debug)]
struct TypingData {
    last_typing_sent: Option<std::time::Instant>,
    typing_users: HashMap<String, HashMap<String, std::time::Instant>>, // channel_id -> user_id -> timestamp
    silent_typing: bool,
}

impl TypingData {
    pub fn new(silent_typing: bool) -> Self {
        Self {
            silent_typing,
            ..Default::default()
        }
    }
}

#[derive(Default, Debug)]
struct Data {
    current_user: Option<User>,
    guilds: GuildData,
    dms: DMData,
    emojis: EmojiData,

    input: InputData,
    status_message: String,
    terminal_dimensions: (usize, usize),

    selection_index: usize,
    scroll_offset: usize,
    cursor_position: usize,

    context: Option<PermissionContext>,
    vim: VimData,
    notifs: NotifsData,
    typing: TypingData,

    display_username: bool,
    logs: Vec<String>,
    log_reader: LogReader,
    deleted_message_ids: HashSet<String>,
}

impl Data {
    pub fn new(
        emojis: EmojiData,
        vim: VimData,
        notifs: NotifsData,
        typing: TypingData,
        log_reader: LogReader,
        display_username: bool,
    ) -> Self {
        Self {
            emojis,
            vim,
            notifs,
            typing,
            log_reader,
            display_username,
            ..Default::default()
        }
    }
}

#[derive(Default, Debug)]
struct App {
    client: Client,
    state: AppState,
    data: Data,

    tick_count: usize,
    is_loading: bool,
}

impl App {
    pub fn new(client: Client, data: Data) -> Self {
        Self {
            client,
            data,
            ..Default::default()
        }
    }
}

async fn run_app(token: String, config: config::Config) -> Result<(), Error> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableBracketedPaste)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (tx_action, mut rx_action) = mpsc::channel::<AppAction>(32);
    let (tx_shutdown, _) = tokio::sync::broadcast::channel::<()>(1);

    let mut path = get_log_directory(APP_NAME).unwrap_or(".".into());
    let _ = std::fs::create_dir_all(&path);
    path.push(LOGS_FOLDER);

    let log_reader = match LogReader::new(path.clone()) {
        Ok(lg) => lg,
        Err(e) => {
            print_log(
                format!("Failed to create LogReader: {e}").into(),
                LogType::Error,
            )
            .await
            .ok();
            return Err(e);
        }
    };

    let client = Client::new(
        ApiClient::new(
            ReqwestClient::new(),
            token.clone(),
            DISCORD_BASE_URL.to_string(),
        ),
        GatewayClient::new(token.clone(), tx_action.clone()),
    );
    let data = Data::new(
        EmojiData::new(config.emoji_map),
        VimData::new(config.vim_mode),
        NotifsData::new(config.discreet_notifs),
        TypingData::new(config.silent_typing),
        log_reader,
        config.display_username,
    );
    let app_state = Arc::new(Mutex::new(App::new(client, data)));

    let tx_ticker = tx_action.clone();
    let mut rx_shutdown_ticker = tx_shutdown.subscribe();

    let ticker_handle: JoinHandle<()> = tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_millis(100));
        loop {
            tokio::select! {
                _ = rx_shutdown_ticker.recv() => {
                    print_log("Shutdown Program.".into(), LogType::Info).await.ok();
                    return;
                }
                _ = interval.tick() => {
                    if let Err(e) = tx_ticker.send(AppAction::Tick).await {
                        print_log(format!("Failed to send tick action: {e}").into(), LogType::Error).await.ok();
                        return;
                    }
                }
            }
        }
    });

    let tx_input = tx_action.clone();
    let rx_shutdown_input = tx_shutdown.subscribe();

    let input_handle: JoinHandle<Result<(), io::Error>> = tokio::spawn(async move {
        let res = handle_input_events(tx_input, rx_shutdown_input).await;
        if let Err(e) = &res {
            print_log(format!("Input Error: {e}").into(), LogType::Error)
                .await
                .ok();
        }
        res
    });

    let tx_logs = tx_action.clone();
    let mut rx_shutdown_logs = tx_shutdown.subscribe();

    let logs_handle: JoinHandle<()> = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = rx_shutdown_logs.recv() => {
                    return;
                }
                Err(e) = watch_logs(path.clone(), tx_logs.clone()) => {
                    print_log(format!("Logs watcher stopped working: {e}").into(), LogType::Error).await.ok();
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
    });

    let api_state = Arc::clone(&app_state);
    let tx_api = tx_action.clone();
    let mut rx_shutdown_api = tx_shutdown.subscribe();

    let rx_shutdown_gateway = tx_shutdown.subscribe();

    let client = api_state.lock().await.client.gateway.clone();
    let gateway_handle: JoinHandle<()> = tokio::spawn(async move {
        if let Err(e) = client.connect(rx_shutdown_gateway).await {
            print_log(
                format!("Gateway connection failed: {e}").into(),
                LogType::Error,
            )
            .await
            .ok();
        }
    });

    let api_handle: JoinHandle<()> = tokio::spawn(async move {
        let api_client_clone;
        {
            let state = api_state.lock().await;
            api_client_clone = state.client.api.clone();
        }

        match api_client_clone.get_current_user().await {
            Ok(user) => {
                if let Err(e) = tx_api.send(AppAction::ApiUpdateCurrentUser(user)).await {
                    print_log(
                        format!("Failed to send current user update action: {e}").into(),
                        LogType::Error,
                    )
                    .await
                    .ok();
                }
            }
            Err(e) => {
                let mut state = api_state.lock().await;
                state.data.status_message = format!("Failed to load current user. {e}");
            }
        }

        match api_client_clone.get_current_user_guilds().await {
            Ok(guilds) => {
                if let Err(e) = tx_api.send(AppAction::ApiUpdateGuilds(guilds)).await {
                    print_log(
                        format!("Failed to send guild update action: {e}").into(),
                        LogType::Error,
                    )
                    .await
                    .ok();
                }
            }
            Err(e) => {
                let mut state = api_state.lock().await;
                print_log(
                    format!("Failed to get user guilds action: {e}").into(),
                    LogType::Error,
                )
                .await
                .ok();
                state.data.status_message = format!("Failed to load servers. {e}");
            }
        }

        match api_client_clone.get_dms().await {
            Ok(dms) => {
                if let Err(e) = tx_api.send(AppAction::ApiUpdateDMs(dms)).await {
                    print_log(
                        format!("Failed to send DM update action: {e}").into(),
                        LogType::Error,
                    )
                    .await
                    .ok();
                }
            }
            Err(e) => {
                let mut state = api_state.lock().await;
                state.data.status_message = format!("Failed to load DMs. {e}");
            }
        }

        tx_api.send(AppAction::EndLoading).await.ok();

        // Wait for shutdown now since HTTP polling is removed
        rx_shutdown_api.recv().await.ok();
    });

    loop {
        {
            let mut state_guard = app_state.lock().await;
            terminal
                .draw(|f| {
                    draw_ui(f, &mut state_guard);
                })
                .unwrap();

            if !state_guard.data.vim.active {
                execute!(io::stdout(), SetCursorStyle::BlinkingBar).ok();
            } else {
                match state_guard.data.input.mode {
                    InputMode::Normal | InputMode::Visual | InputMode::VisualLine => {
                        execute!(io::stdout(), SetCursorStyle::BlinkingBlock).ok();
                    }
                    InputMode::Insert | InputMode::Command | InputMode::Search => {
                        execute!(io::stdout(), SetCursorStyle::BlinkingBar).ok();
                    }
                }
            }
        }
        if let Some(action) = rx_action.recv().await {
            let state = app_state.lock().await;

            match handle_keys_events(state, action, tx_action.clone()).await {
                Some(KeywordAction::Continue) => continue,
                Some(KeywordAction::Break) => break,
                None => {}
            }
        }
    }

    drop(rx_action);

    tx_shutdown.send(()).ok();

    let _ = tokio::join!(
        input_handle,
        api_handle,
        ticker_handle,
        logs_handle,
        gateway_handle
    );

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    print_log("Launching Program...".into(), LogType::Info)
        .await
        .ok();
    dotenvy::dotenv().ok();
    const ENV_TOKEN: &str = "DISCORD_TOKEN";

    let token = match env::var(ENV_TOKEN) {
        Ok(token) => token,
        Err(e) => {
            eprintln!("{e}");
            print_log(e.into(), LogType::Error).await.ok();
            process::exit(1);
        }
    };

    setup_ctrlc_handler();

    let config = config::load_config().await;

    if let Err(e) = run_app(token, config).await {
        restore_terminal();
        return Err(e);
    }

    restore_terminal();

    Ok(())
}
