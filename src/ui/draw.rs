use ratatui::{
    style::{Color, Style, Stylize},
    text::Span,
    widgets::{BorderType, Clear, List, ListItem, ListState},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    App, AppState, InputMode,
    api::{Channel, DM, Emoji, guild::PartialGuild},
};

pub fn draw_ui(f: &mut ratatui::Frame, app: &mut App) {
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::text::{Line, Text};
    use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

    let area = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(90), Constraint::Percentage(10)].as_ref())
        .split(area);

    app.data.terminal_dimensions.1 = chunks[0].height as usize;
    app.data.terminal_dimensions.0 = chunks[0].width as usize;

    let max_height = app.data.terminal_dimensions.1.saturating_sub(2);
    let max_width = app.data.terminal_dimensions.0.saturating_sub(2) as u16;

    match &app.state {
        AppState::Loading(_) => {
            let loading_area = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(40),
                    Constraint::Length(3),
                    Constraint::Min(0),
                ])
                .split(chunks[0])[1];

            let spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let symbol = spinner[app.tick_count % spinner.len()];

            let loading_text = Line::from(vec![
                Span::styled("Loading", Style::default().fg(Color::LightCyan)),
                Span::raw(" "),
                Span::styled(symbol, Style::default().fg(Color::LightCyan)),
            ]);

            let loading_paragraph = Paragraph::new(Text::from(vec![loading_text]))
                .alignment(ratatui::layout::Alignment::Center)
                .block(Block::default().borders(Borders::NONE));

            f.render_widget(Clear, chunks[0]);
            f.render_widget(loading_paragraph, loading_area);
        }
        AppState::Home => {
            let options = [
                ("Guilds", Color::LightMagenta),
                ("DMs", Color::LightYellow),
                ("Quit", Color::LightRed),
            ];

            let items: Vec<ListItem> = options.iter().map(|o| ListItem::new(o.0).fg(o.1)).collect();

            let list = List::new(items)
                .block(
                    Block::default()
                        .title(Span::styled(
                            "vimcord Client - Home",
                            Style::default().fg(Color::Yellow),
                        ))
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double),
                )
                .highlight_style(Style::default().reversed())
                .highlight_symbol(">> ");

            app.data.selection_index = app
                .data
                .selection_index
                .min(options.len().saturating_sub(1));

            let mut state = ListState::default().with_selected(Some(app.data.selection_index));
            f.render_widget(Clear, chunks[0]);
            f.render_stateful_widget(list, chunks[0], &mut state);
        }
        AppState::SelectingDM => {
            let filter_text = app.data.input.search.to_lowercase();

            let filtered_dms: Vec<&DM> = app
                .data
                .dms
                .channels
                .iter()
                .filter(|d| d.get_name().to_lowercase().contains(&filter_text))
                .collect();

            let items: Vec<ListItem> = filtered_dms
                .iter()
                .map(|d| {
                    let mut spans = Vec::new();

                    if d.channel_type == 1 && d.recipients.len() == 1 {
                        let (status_char, status_color) = match app
                            .data
                            .dms
                            .user_statuses
                            .get(&d.recipients[0].id)
                            .map(|s| s.as_str())
                        {
                            Some("online") => ("", Color::LightGreen),
                            Some("idle") => ("", Color::LightYellow),
                            Some("dnd") => ("", Color::LightRed),
                            _ => ("", Color::DarkGray), // offline/invisible/unknown
                        };
                        spans.push(Span::styled(
                            format!("{} ", status_char),
                            Style::default().fg(status_color),
                        ));
                    }

                    let char = match d.channel_type {
                        1 => '',
                        3 => '',
                        _ => '',
                    };

                    let color = match d.channel_type {
                        1 => Color::LightMagenta,
                        3 => Color::LightBlue,
                        _ => Color::LightRed,
                    };

                    let name_text = format!("{char} {}", d.get_name());
                    spans.push(Span::styled(name_text.clone(), Style::default().fg(color)));

                    // Show custom status text next to the username for 1:1 DMs
                    if d.channel_type == 1
                        && d.recipients.len() == 1
                        && let Some(status_text) =
                            app.data.dms.user_status_texts.get(&d.recipients[0].id)
                        && !status_text.is_empty()
                    {
                        spans.push(Span::styled(
                            format!(" - {}", status_text),
                            Style::default().fg(Color::Gray),
                        ));
                    }

                    ListItem::new(Line::from(spans))
                })
                .collect();

            let num_filtered = items.len();
            app.data.selection_index = app.data.selection_index.min(num_filtered.saturating_sub(1));

            let list = List::new(items)
                .block(
                    Block::default()
                        .title(Span::styled(
                            "vimcord Client - Direct Messages",
                            Style::default().fg(Color::Yellow),
                        ))
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double),
                )
                .highlight_style(Style::default().reversed())
                .highlight_symbol(">> ");

            let mut state = ListState::default().with_selected(Some(app.data.selection_index));
            f.render_widget(Clear, chunks[0]);
            f.render_stateful_widget(list, chunks[0], &mut state);
        }
        AppState::SelectingGuild => {
            let filter_text = app.data.input.search.to_lowercase();

            let filtered_guilds: Vec<&PartialGuild> = app
                .data
                .guilds
                .joined
                .iter()
                .filter(|g| g.name.to_lowercase().contains(&filter_text))
                .collect();

            let mut count = 0;
            let items: Vec<ListItem> = filtered_guilds
                .iter()
                .map(|g| {
                    let color = if count % 2 == 0 {
                        Color::LightCyan
                    } else {
                        Color::LightYellow
                    };

                    count += 1;

                    ListItem::new(g.name.as_str()).style(Style::default().fg(color))
                })
                .collect();

            let num_filtered = items.len();
            app.data.selection_index = app.data.selection_index.min(num_filtered.saturating_sub(1));

            let list = List::new(items)
                .block(
                    Block::default()
                        .title(Span::styled(
                            "vimcord Client - Guilds",
                            Style::default().fg(Color::Yellow),
                        ))
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double),
                )
                .highlight_style(Style::default().reversed())
                .highlight_symbol(">> ");

            let mut state = ListState::default().with_selected(Some(app.data.selection_index));
            f.render_widget(Clear, chunks[0]);
            f.render_stateful_widget(list, chunks[0], &mut state);
        }
        AppState::SelectingChannel(guild) => {
            let filter_text = app.data.input.search.to_lowercase();

            let permission_context = &app.data.context;

            let mut list_items: Vec<ListItem> = Vec::new();

            let should_display_channel_content = |c: &Channel| {
                let is_readable = permission_context
                    .as_ref()
                    .is_some_and(|context| c.is_readable(context));

                is_readable
                    && (filter_text.is_empty() || c.name.to_lowercase().contains(&filter_text))
            };

            app.data
                .guilds
                .channels
                .iter()
                .filter(|c| {
                    if c.children.is_none() && c.channel_type != 4 {
                        return should_display_channel_content(c);
                    }

                    if c.channel_type == 4 {
                        if filter_text.is_empty() || c.name.to_lowercase().contains(&filter_text) {
                            return true;
                        }

                        if let Some(children) = &c.children {
                            return children.iter().any(should_display_channel_content);
                        }
                    }

                    false
                })
                .for_each(|c| {
                    let get_channel_style = |channel_type: u8| -> (char, Color) {
                        match channel_type {
                            15 => ('', Color::LightYellow),
                            13 => ('󱝉', Color::LightRed),
                            5 => ('', Color::LightGreen),
                            4 => ('', Color::Gray),
                            2 => ('', Color::LightCyan),
                            0 => ('', Color::LightBlue),
                            _ => ('', Color::LightMagenta),
                        }
                    };

                    if c.channel_type == 4 {
                        let (char, color) = get_channel_style(c.channel_type);
                        list_items.push(
                            ListItem::new(format!("{char} {}", c.name))
                                .style(Style::default().fg(color)),
                        );

                        if let Some(children) = &c.children {
                            children
                                .iter()
                                .filter(|c| should_display_channel_content(c))
                                .for_each(|child| {
                                    let (char, color) = get_channel_style(child.channel_type);

                                    list_items.push(
                                        ListItem::new(format!("  {char} {}", child.name))
                                            .style(Style::default().fg(color)),
                                    );
                                });
                        }
                    } else {
                        let (char, color) = get_channel_style(c.channel_type);
                        list_items.push(
                            ListItem::new(format!("{char} {}", c.name))
                                .style(Style::default().fg(color)),
                        );
                    }
                });

            let num_filtered = list_items.len();
            app.data.selection_index = app.data.selection_index.min(num_filtered.saturating_sub(1));

            let hidden_items: Vec<ListItem> = app
                .data
                .guilds
                .channels
                .iter()
                .flat_map(|c| {
                    if c.channel_type == 4 {
                        let mut items: Vec<&Channel> = Vec::new();

                        if let Some(children) = &c.children {
                            items.extend(children.iter().filter(|child| {
                                permission_context
                                    .as_ref()
                                    .is_some_and(|context| !child.is_readable(context))
                            }));
                        }
                        items
                    } else if permission_context
                        .as_ref()
                        .is_some_and(|context| !c.is_readable(context))
                    {
                        vec![c]
                    } else {
                        vec![]
                    }
                })
                .map(|c| {
                    let char = match c.channel_type {
                        15 => '',
                        13 => '󱝉',
                        5 => '',
                        4 => '',
                        2 => '',
                        0 => '',
                        _ => '',
                    };

                    let color = Color::DarkGray;

                    ListItem::new(format!(" {char} {}", c.name)).style(Style::default().fg(color))
                })
                .collect();

            list_items.extend(hidden_items);

            let title = format!(
                "Channels for Guild: {} | Channels found: {} | Actual index: {}",
                guild.name,
                num_filtered.saturating_sub(1),
                app.data.selection_index
            );

            let list = List::new(list_items)
                .block(
                    Block::default()
                        .title(Span::styled(title, Style::default().fg(Color::Yellow)))
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double),
                )
                .highlight_style(Style::default().reversed())
                .highlight_symbol(">> ");

            let mut state = ListState::default().with_selected(Some(app.data.selection_index));
            f.render_widget(Clear, chunks[0]);
            f.render_stateful_widget(list, chunks[0], &mut state);
        }
        AppState::Chatting(channel)
        | AppState::EmojiSelection(channel)
        | AppState::Editing(channel, _, _) => {
            if max_width == 0 {
                return;
            }

            let mut messages_reversed_with_index = app
                .data
                .guilds
                .messages
                .iter()
                .filter(|m| {
                    m.content
                        .clone()
                        .unwrap_or_default()
                        .to_lowercase()
                        .contains(app.data.input.search.to_lowercase().as_str())
                })
                .enumerate()
                .collect::<Vec<_>>();
            messages_reversed_with_index.reverse(); // Oldest first

            let mut final_content: Vec<Line> = Vec::new();
            let mut total_visual_height = 0;

            for (original_idx, message) in messages_reversed_with_index.into_iter() {
                let is_selected =
                    app.data.selection_index > 0 && app.data.selection_index - 1 == original_idx;

                let author = if app.data.display_username {
                    message.author.username.clone()
                } else {
                    message
                        .author
                        .global_name
                        .clone()
                        .unwrap_or(message.author.username.clone())
                };

                let formatted_text = format!(
                    "[{}] {}: {}",
                    message
                        .timestamp
                        .split('T')
                        .next()
                        .unwrap_or("")
                        .to_string()
                        + " "
                        + message
                            .timestamp
                            .split('T')
                            .nth(1)
                            .unwrap_or("")
                            .split('.')
                            .next()
                            .unwrap_or(""),
                    author,
                    message.content.as_deref().unwrap_or("(*non-text*)")
                );

                let text_lines: Vec<&str> = formatted_text.split('\n').collect();
                let mut estimated_height = 0;

                let safe_max_width = max_width.saturating_sub(4);
                for line in text_lines {
                    let width = UnicodeWidthStr::width(line);

                    if width == 0 || safe_max_width == 0 {
                        estimated_height += 1;
                        continue;
                    }

                    let mut current_line_width = 0;
                    let mut first_word = true;

                    for word in line.split(' ') {
                        let word_width = UnicodeWidthStr::width(word);
                        let space_width = if first_word { 0 } else { 1 };

                        if current_line_width + space_width + word_width <= safe_max_width as usize
                        {
                            current_line_width += space_width + word_width;
                        } else {
                            if current_line_width > 0 {
                                estimated_height += 1;
                            }

                            if word_width > safe_max_width as usize {
                                let chunks = word_width.div_ceil(safe_max_width as usize);
                                estimated_height += chunks.saturating_sub(1);
                                current_line_width = word_width % safe_max_width as usize;
                                if current_line_width == 0 {
                                    current_line_width = safe_max_width as usize;
                                }
                            } else {
                                current_line_width = word_width;
                            }
                        }
                        first_word = false;
                    }
                    if current_line_width > 0 {
                        estimated_height += 1;
                    }
                }

                let start_y = total_visual_height;
                total_visual_height += estimated_height;
                let end_y = total_visual_height;

                if is_selected {
                    if start_y < app.data.scroll_offset {
                        app.data.scroll_offset = start_y;
                    } else if end_y > app.data.scroll_offset + max_height {
                        app.data.scroll_offset = end_y.saturating_sub(max_height);
                    }
                }

                let formatted_time = format!(
                    " {}]",
                    message
                        .timestamp
                        .split('T')
                        .nth(1)
                        .unwrap_or("")
                        .split('.')
                        .next()
                        .unwrap_or(""),
                );

                let formatted_date = message
                    .timestamp
                    .split('T')
                    .next()
                    .unwrap_or("")
                    .to_string();

                let name = if app.data.display_username {
                    message.author.username.clone()
                } else {
                    message
                        .author
                        .global_name
                        .clone()
                        .unwrap_or(message.author.username.clone())
                };

                let author = format!(" {name}: ");

                let content;
                if let Some(guild) = app.data.guilds.selected.clone() {
                    content = message.map_mentions(Some(guild));
                } else {
                    content = message.map_mentions(None);
                }

                let content_lines: Vec<&str> = content.split('\n').collect();

                let mentionned = if let Some(author) = &app.data.current_user {
                    message.mentions.contains(author)
                } else {
                    false
                };

                let bg_color = if is_selected {
                    Color::DarkGray
                } else {
                    Color::Reset
                };

                let mut style = Style::default().fg(Color::White).bg(bg_color);

                if mentionned {
                    style = style.reversed();
                }

                for (i, line_content) in content_lines.iter().enumerate() {
                    let mut spans = vec![];

                    if i == 0 {
                        spans.push(Span::styled(
                            "[".to_string(),
                            Style::default().fg(Color::LightBlue).bg(bg_color),
                        ));
                        spans.push(Span::styled(
                            formatted_date.clone(),
                            Style::default().fg(Color::LightCyan).bg(bg_color),
                        ));
                        spans.push(Span::styled(
                            formatted_time.clone(),
                            Style::default().fg(Color::LightBlue).bg(bg_color),
                        ));
                        spans.push(Span::styled(
                            author.clone(),
                            Style::default().fg(Color::Yellow).bg(bg_color),
                        ));
                    } else {
                        // Keep multi-line messages highlighted properly across all lines
                        spans.push(Span::styled("".to_string(), Style::default().bg(bg_color)));
                    }

                    spans.push(Span::styled(line_content.to_string(), style));
                    final_content.push(Line::from(spans));
                }
            }

            if app.data.selection_index == 0 {
                app.data.scroll_offset = total_visual_height.saturating_sub(max_height);
            }

            let mut title_spans = vec![Span::styled(
                "vimcord Client - Chatting in channel - ",
                Style::default().fg(Color::Yellow),
            )];

            if let crate::api::AnyChannel::Direct(d) = &**channel
                && d.channel_type == 1
                && d.recipients.len() == 1
            {
                let (status_char, status_color) = match app
                    .data
                    .dms
                    .user_statuses
                    .get(&d.recipients[0].id)
                    .map(|s| s.as_str())
                {
                    Some("online") => ("", Color::LightGreen),
                    Some("idle") => ("", Color::LightYellow),
                    Some("dnd") => ("", Color::LightRed),
                    _ => ("", Color::DarkGray), // offline/invisible/unknown
                };
                title_spans.push(Span::styled(
                    format!("{} ", status_char),
                    Style::default().fg(status_color),
                ));
            }
            title_spans.push(Span::styled(
                channel.get_name(),
                Style::default().fg(Color::Yellow),
            ));

            if let crate::api::AnyChannel::Direct(d) = &**channel
                && d.channel_type == 1
                && d.recipients.len() == 1
                && let Some(status_text) = app.data.dms.user_status_texts.get(&d.recipients[0].id)
                && !status_text.is_empty()
            {
                title_spans.push(Span::styled(
                    format!(" - {}", status_text),
                    Style::default().fg(Color::Gray),
                ));
            }

            let title = Line::from(title_spans);

            let paragraph = Paragraph::new(final_content)
                .block(
                    Block::default()
                        .title(title)
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double),
                )
                .wrap(Wrap { trim: false })
                .scroll((app.data.scroll_offset as u16, 0));

            f.render_widget(Clear, chunks[0]);
            f.render_widget(paragraph, chunks[0]);
        }
        AppState::Logs(_) => {
            if max_width == 0 {
                return;
            }

            let mut log_messages = app
                .data
                .logs
                .iter()
                .filter(|m| {
                    m.to_lowercase()
                        .contains(app.data.input.search.to_lowercase().as_str())
                })
                .enumerate()
                .collect::<Vec<_>>();
            log_messages.reverse();

            let mut final_content: Vec<Line> = Vec::new();
            let mut total_visual_height = 0;

            for (original_idx, message) in log_messages.into_iter() {
                let is_selected =
                    app.data.selection_index > 0 && app.data.selection_index - 1 == original_idx;

                let text_lines: Vec<&str> = message.split('\n').collect();
                let mut estimated_height = 0;

                let safe_max_width = max_width.saturating_sub(4);
                for line in text_lines {
                    let width = UnicodeWidthStr::width(line);

                    if width == 0 || safe_max_width == 0 {
                        estimated_height += 1;
                        continue;
                    }

                    let mut current_line_width = 0;
                    let mut first_word = true;

                    for word in line.split(' ') {
                        let word_width = UnicodeWidthStr::width(word);
                        let space_width = if first_word { 0 } else { 1 };

                        if current_line_width + space_width + word_width <= safe_max_width as usize
                        {
                            current_line_width += space_width + word_width;
                        } else {
                            if current_line_width > 0 {
                                estimated_height += 1;
                            }

                            if word_width > safe_max_width as usize {
                                let chunks = word_width.div_ceil(safe_max_width as usize);
                                estimated_height += chunks.saturating_sub(1);
                                current_line_width = word_width % safe_max_width as usize;
                                if current_line_width == 0 {
                                    current_line_width = safe_max_width as usize;
                                }
                            } else {
                                current_line_width = word_width;
                            }
                        }
                        first_word = false;
                    }
                    if current_line_width > 0 {
                        estimated_height += 1;
                    }
                }

                let start_y = total_visual_height;
                total_visual_height += estimated_height;
                let end_y = total_visual_height;

                if is_selected {
                    if start_y < app.data.scroll_offset {
                        app.data.scroll_offset = start_y;
                    } else if end_y > app.data.scroll_offset + max_height {
                        app.data.scroll_offset = end_y.saturating_sub(max_height);
                    }
                }

                let bg_color = if is_selected {
                    Color::DarkGray
                } else {
                    Color::Reset
                };

                fn parse_log(log_line: &str) -> Option<(&str, &str, &str, &str)> {
                    let bracket_end = log_line.find(']')?;
                    let datetime = &log_line[1..bracket_end];
                    let (date, time) = datetime.split_once(' ')?;
                    let rest = log_line[bracket_end + 1..].trim_start();
                    let (log_type, content) = rest.split_once(": ")?;

                    Some((date, time, log_type, content))
                }

                let (date, time, log_type, content) =
                    parse_log(message.as_str()).unwrap_or_default();

                let content_lines: Vec<&str> = content.split('\n').collect();

                let style = Style::default().fg(Color::White).bg(bg_color);

                let log_type_color = match log_type {
                    "ERROR" => Color::Red,
                    "WARN" => Color::Yellow,
                    "INFO" => Color::Cyan,
                    "DEBUG" => Color::Green,
                    _ => Color::Green,
                };

                for (i, line_content) in content_lines.iter().enumerate() {
                    let mut spans = vec![];

                    if i == 0 {
                        spans.push(Span::styled(
                            "[",
                            Style::default().fg(Color::LightBlue).bg(bg_color),
                        ));
                        spans.push(Span::styled(
                            date,
                            Style::default().fg(Color::LightCyan).bg(bg_color),
                        ));
                        spans.push(Span::default().content(" ").bg(bg_color));
                        spans.push(Span::styled(
                            time,
                            Style::default().fg(Color::LightBlue).bg(bg_color),
                        ));
                        spans.push(Span::styled(
                            "]",
                            Style::default().fg(Color::LightBlue).bg(bg_color),
                        ));
                        spans.push(Span::default().content(" ").bg(bg_color));
                        spans.push(Span::styled(
                            log_type,
                            Style::default().fg(log_type_color).bg(bg_color),
                        ));
                        spans.push(Span::default().content(" ").bg(bg_color));
                    } else {
                        // Keep multi-line messages highlighted properly across all lines
                        spans.push(Span::styled("".to_string(), Style::default().bg(bg_color)));
                    }

                    spans.push(Span::styled(line_content.to_string(), style));
                    final_content.push(Line::from(spans));
                }
            }

            if app.data.selection_index == 0 {
                app.data.scroll_offset = total_visual_height.saturating_sub(max_height);
            }

            let title = format!(
                "vimcord Client - Logs ({}/{})",
                app.data.selection_index,
                app.data.logs.len()
            );

            let paragraph = Paragraph::new(final_content)
                .block(
                    Block::default()
                        .title(Span::styled(title, Style::default().fg(Color::Yellow)))
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double),
                )
                .wrap(Wrap { trim: false })
                .scroll((app.data.scroll_offset as u16, 0));

            f.render_widget(Clear, chunks[0]);
            f.render_widget(paragraph, chunks[0]);
        }
    };

    if let AppState::EmojiSelection(_) = &app.state {
        let input_area = chunks[1];
        let emoji_popup_height = 8;

        let popup_rect = ratatui::layout::Rect {
            x: input_area
                .width
                .saturating_sub(input_area.width.saturating_div(4)),
            y: input_area.y.saturating_sub(emoji_popup_height + 1),
            width: input_area.width.saturating_sub(2).saturating_div(4),
            height: emoji_popup_height,
        };

        f.render_widget(Clear, popup_rect);

        let mut filtered_items: Vec<ListItem> = Vec::new();

        let filtered_unicode: Vec<&(String, String)> = app
            .data
            .emojis
            .map
            .iter()
            .filter(|(name, _)| name.starts_with(&app.data.emojis.filter))
            .collect();

        let filtered_custom: Vec<&Emoji> = app
            .data
            .guilds
            .custom_emojis
            .iter()
            .filter(|e| e.name.starts_with(&app.data.emojis.filter))
            .collect();

        for (name, char) in filtered_unicode.iter() {
            filtered_items.push(ListItem::new(Line::from(vec![
                Span::styled(char.clone(), Style::default().fg(Color::White)),
                Span::raw(" "),
                Span::styled(
                    format!(":{name}: (Unicode)"),
                    Style::default().fg(Color::LightBlue),
                ),
            ])));
        }

        for emoji in filtered_custom.iter() {
            filtered_items.push(ListItem::new(Line::from(vec![Span::styled(
                format!("  :{}: (Guild)", emoji.name),
                Style::default().fg(Color::LightBlue),
            )])));
        }

        if !filtered_items.is_empty() {
            app.data.emojis.index = app
                .data
                .emojis
                .index
                .min(filtered_items.len().saturating_sub(1));

            let emoji_list = List::new(filtered_items)
                .block(
                    Block::default()
                        .title(Span::styled(
                            "Select An Emoji",
                            Style::default().fg(Color::Yellow),
                        ))
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double),
                )
                .highlight_style(Style::default().reversed())
                .highlight_symbol(">> ");

            let mut state = ListState::default().with_selected(Some(app.data.emojis.index));
            f.render_stateful_widget(emoji_list, popup_rect, &mut state);
        } else {
            app.data.emojis.index = 0;
        }
    }

    let is_editing = matches!(&app.state, AppState::Editing(_, _, _));
    let border_color = if is_editing {
        Color::LightMagenta
    } else if let InputMode::Command = &app.data.input.mode {
        Color::LightGreen
    } else if let InputMode::Search = &app.data.input.mode {
        Color::LightRed
    } else {
        Color::Reset
    };
    let title_color = if is_editing {
        Color::LightMagenta
    } else if let InputMode::Command = &app.data.input.mode {
        Color::LightGreen
    } else if let InputMode::Search = &app.data.input.mode {
        Color::LightRed
    } else {
        Color::Yellow
    };

    let mut display_status_message = app.data.status_message.clone();

    let active_channel_id = match &app.state {
        AppState::Chatting(channel) => Some(channel.get_id()),
        AppState::EmojiSelection(channel) => Some(channel.get_id()),
        AppState::Editing(channel, _, _) => Some(channel.get_id()),
        _ => None,
    };

    if let Some(channel_id) = active_channel_id
        && let Some(typers) = app.data.typing.typing_users.get(&channel_id)
        && !typers.is_empty()
    {
        let mut typers_names = Vec::new();
        for id in typers.keys() {
            let name = app
                .data
                .dms
                .user_names
                .get(id)
                .cloned()
                .unwrap_or_else(|| "Someone".to_string());
            typers_names.push(name);
        }

        let text = if typers_names.len() > 3 {
            "Several people are typing...".to_string()
        } else {
            let names = typers_names.join(", ");
            if typers_names.len() == 1 {
                format!("{names} is typing...")
            } else {
                format!("{names} are typing...")
            }
        };

        display_status_message = format!("{} | {}", app.data.status_message, text);
    }

    let title = match &app.data.input.mode {
        InputMode::Command => "Command Line".to_string(),
        InputMode::Search => "Searching".to_string(),
        _ => format!("Input: {}", display_status_message),
    };

    let input_text = if app.data.input.mode == InputMode::Visual
        || app.data.input.mode == InputMode::VisualLine
    {
        if let Some(vim_state) = &app.data.vim.state {
            if let Some(visual_start) = vim_state.visual_start {
                let mut start = visual_start.min(app.data.cursor_position);
                let mut end = visual_start.max(app.data.cursor_position);
                let end_len = app.data.input.buffer[end..]
                    .chars()
                    .next()
                    .map(|c| c.len_utf8())
                    .unwrap_or(0);
                end = (end + end_len).min(app.data.input.buffer.len());

                if app.data.input.mode == InputMode::VisualLine {
                    start = app.data.input.buffer[..start]
                        .rfind('\n')
                        .map(|i| i + 1)
                        .unwrap_or(0);
                    end = app.data.input.buffer[end..]
                        .find('\n')
                        .map(|i| end + i + 1)
                        .unwrap_or(app.data.input.buffer.len());
                }

                let mut lines = Vec::new();
                let mut current_pos = 0;

                for line in app.data.input.buffer.split('\n') {
                    let line_start = current_pos;
                    let line_end = current_pos + line.len();

                    let mut spans = vec![];
                    let overlap_start = start.max(line_start);
                    let overlap_end = end.min(line_end);

                    if overlap_start < overlap_end {
                        if line_start < overlap_start {
                            spans
                                .push(Span::raw(&app.data.input.buffer[line_start..overlap_start]));
                        }
                        spans.push(Span::styled(
                            &app.data.input.buffer[overlap_start..overlap_end],
                            Style::default().reversed(),
                        ));
                        if overlap_end < line_end {
                            spans.push(Span::raw(&app.data.input.buffer[overlap_end..line_end]));
                        }
                    } else {
                        spans.push(Span::raw(&app.data.input.buffer[line_start..line_end]));
                    }

                    lines.push(Line::from(spans));
                    current_pos = line_end + 1;
                }
                Text::from(lines)
            } else {
                Text::from(app.data.input.buffer.as_str())
            }
        } else {
            Text::from(app.data.input.buffer.as_str())
        }
    } else {
        Text::from(app.data.input.buffer.as_str())
    };

    f.render_widget(
        Paragraph::new(input_text).block(
            Block::default()
                .title(Span::styled(title, Style::default().fg(title_color)))
                .borders(Borders::ALL)
                .border_type(BorderType::Double)
                .border_style(Style::default().fg(border_color)),
        ),
        chunks[1],
    );

    if app.data.selection_index == 0 {
        let cursor = if app.data.cursor_position <= app.data.input.buffer.len()
            && app.data.cursor_position > 0
        {
            app.data.cursor_position
        } else {
            0
        };
        let input_before_cursor = &app.data.input.buffer[..cursor];
        let cursor_lines = input_before_cursor.split('\n').count();
        let cursor_y = chunks[1].y + cursor_lines as u16;

        let current_line_start = input_before_cursor.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let cursor_x = chunks[1].x
            + 1
            + UnicodeWidthStr::width(&input_before_cursor[current_line_start..]) as u16;

        f.set_cursor_position((cursor_x, cursor_y));
    }
}
