use crate::{APP_NAME, AppAction, Error};

use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, SeekFrom},
    time::{self, Duration},
};

use std::{fmt::Display, path::PathBuf};

pub enum LogType {
    Error,
    #[allow(dead_code)]
    Warning,
    Info,
    Debug,
}

impl Display for LogType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let desc = match *self {
            LogType::Error => "ERROR",
            LogType::Warning => "WARN",
            LogType::Info => "INFO",
            LogType::Debug => "DEBUG",
        };
        f.write_str(desc)
    }
}

#[derive(Debug, Default)]
pub struct LogReader {
    path: PathBuf,
    cursor: u64,
}

impl LogReader {
    pub fn new(path: PathBuf) -> Result<Self, Error> {
        let metadata = std::fs::metadata(&path)?;
        Ok(Self {
            path,
            cursor: metadata.len(),
        })
    }

    pub async fn read_previous_lines(&mut self, num_lines: usize) -> Result<Vec<String>, Error> {
        let mut file = File::open(&self.path).await?;
        let mut lines = Vec::new();
        let mut buffer = Vec::new();

        let chunk_size = 1024;

        while lines.len() < num_lines && self.cursor > 0 {
            let to_read = std::cmp::min(self.cursor, chunk_size);
            self.cursor -= to_read;

            file.seek(SeekFrom::Start(self.cursor)).await?;
            let mut chunk = vec![0; to_read as usize];
            file.read_exact(&mut chunk).await?;

            chunk.extend_from_slice(&buffer);
            buffer = chunk;

            while let Some(pos) = buffer.iter().rposition(|&b| b == b'\n') {
                if lines.len() >= num_lines {
                    break;
                }

                let line_byte = buffer.split_off(pos);
                if line_byte.len() > 1 {
                    let s = String::from_utf8_lossy(&line_byte[1..]).to_string();
                    if !s.is_empty() {
                        lines.push(s);
                    }
                }
            }
        }

        if self.cursor == 0 && !buffer.is_empty() && lines.len() < num_lines {
            lines.push(String::from_utf8_lossy(&buffer).to_string());
        }

        Ok(lines)
    }
}

pub async fn watch_logs(
    path: PathBuf,
    action_tx: tokio::sync::mpsc::Sender<AppAction>,
) -> Result<(), Error> {
    let mut file = File::open(&path).await?;

    let mut last_position = file.metadata().await?.len();
    file.seek(SeekFrom::Start(last_position)).await?;

    let mut buffer = Vec::new();
    let mut interval = time::interval(Duration::from_millis(100));

    loop {
        interval.tick().await;

        let meta = file.metadata().await?;
        let current_len = meta.len();

        if current_len > last_position {
            let mut new_bytes = Vec::new();
            file.read_to_end(&mut new_bytes).await?;

            buffer.extend_from_slice(&new_bytes);

            while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                let line_bytes = buffer.drain(..=pos).collect::<Vec<u8>>();
                let line = String::from_utf8_lossy(&line_bytes).trim().to_string();

                if !line.is_empty() {
                    action_tx.send(AppAction::NewLogReceived(line)).await.ok();
                }
            }

            last_position = current_len;
        } else if current_len < last_position {
            last_position = 0;
            file.seek(SeekFrom::Start(0)).await?;
            buffer.clear();
            action_tx.send(AppAction::ClearLogs).await.ok();
        }
    }
}

async fn write_log_file(path: PathBuf, msg: &[u8]) -> Result<(), Error> {
    File::options()
        .append(true)
        .create(true)
        .open(path)
        .await?
        .write_all(msg)
        .await?;
    Ok(())
}

pub async fn print_log(msg: Error, log_type: LogType) -> Result<(), Error> {
    let timestamp = chrono::offset::Local::now();
    let msg = format!("[{timestamp}] {log_type}: {msg}\n");
    let mut path = get_log_directory(APP_NAME).unwrap_or(".".into());
    let _ = std::fs::create_dir_all(&path);
    path.push("logs");

    write_log_file(path, msg.as_bytes()).await?;
    Ok(())
}

pub fn get_log_directory(app_name: &str) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        // Windows: %LOCALAPPDATA%\vimcord\logs
        dirs::data_local_dir().map(|mut path| {
            path.push(app_name);
            path
        })
    }

    #[cfg(target_os = "macos")]
    {
        // macOS: ~/Library/Logs/vimcord/logs
        dirs::home_dir().map(|mut path| {
            path.push("Library");
            path.push("Logs");
            path.push(app_name);
            path
        })
    }

    #[cfg(target_os = "linux")]
    {
        // Linux: ~/.cache/vimcord/logs
        let mut path = dirs::cache_dir()?;

        path.push(app_name);
        Some(path)
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        std::env::current_dir().ok()
    }
}
