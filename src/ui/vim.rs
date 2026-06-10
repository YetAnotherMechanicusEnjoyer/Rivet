use std::time::Instant;
use tokio::sync::{MutexGuard, mpsc::Sender};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{
    App, AppAction, AppState, InputMode,
    api::{Channel, DM, guild::PartialGuild},
    logs::{LogType, print_log},
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VimOperator {
    Delete,
    _Change,
    _Yank,
    Goto,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VimMotion {
    WordForward,
    WordBackward,
    _Line,
    _CharRight,
    _CharLeft,
    _StartOfLine,
    _EndOfLine,
}

#[derive(Debug, Clone)]
pub struct VimState {
    pub operator: Option<VimOperator>,
    pub pending_keys: String,
    pub last_action_time: Instant,
    pub visual_start: Option<usize>,
    pub yank_buffer: String,
}

impl Default for VimState {
    fn default() -> Self {
        Self {
            operator: None,
            pending_keys: String::new(),
            last_action_time: Instant::now(),
            visual_start: None,
            yank_buffer: String::new(),
        }
    }
}

pub fn clamp_cursor(state: &mut MutexGuard<'_, App>) {
    let len = state.data.input.buffer.len();
    if len == 0 {
        state.data.cursor_position = 0;
    } else if state.data.cursor_position >= len {
        let last_char_len = state
            .data
            .input
            .buffer
            .chars()
            .last()
            .map(|c| c.len_utf8())
            .unwrap_or(0);
        state.data.cursor_position = len.saturating_sub(last_char_len);
    }
}

fn get_motion_range(state: &MutexGuard<'_, App>, motion: VimMotion) -> (usize, usize) {
    let start = state.data.cursor_position;
    let len = state.data.input.buffer.len();
    let input = &state.data.input.buffer;

    let end = match motion {
        VimMotion::WordForward => {
            let mut pos = start;
            if let Some(c) = input[pos..].chars().next() {
                if c.is_whitespace() {
                    // Cursor is on whitespace: skip whitespace only, stop at start of next word
                    while pos < len {
                        if let Some(c) = input[pos..].chars().next()
                            && c.is_whitespace()
                        {
                            pos += c.len_utf8();
                        } else {
                            break;
                        }
                    }
                } else {
                    // Cursor is on a word/non-whitespace: skip rest of this word...
                    while pos < len {
                        if let Some(c) = input[pos..].chars().next()
                            && !c.is_whitespace()
                        {
                            pos += c.len_utf8();
                        } else {
                            break;
                        }
                    }
                    // ...then skip following whitespace to land at start of next word
                    while pos < len {
                        if let Some(c) = input[pos..].chars().next()
                            && c.is_whitespace()
                        {
                            pos += c.len_utf8();
                        } else {
                            break;
                        }
                    }
                }
            }
            pos.min(len)
        }
        VimMotion::WordBackward => {
            let mut pos = start;
            if pos == 0 {
                return (start, 0);
            }

            // First, check what character is immediately before the cursor
            let prev_char = input[..pos].chars().next_back();

            if let Some(c) = prev_char {
                if c.is_whitespace() {
                    // We're after whitespace - skip all whitespace backwards
                    while pos > 0 {
                        if let Some(c) = input[..pos].chars().next_back()
                            && c.is_whitespace()
                        {
                            pos -= c.len_utf8();
                        } else {
                            break;
                        }
                    }
                    // Now skip the word backwards to find its beginning
                    while pos > 0 {
                        if let Some(c) = input[..pos].chars().next_back()
                            && !c.is_whitespace()
                        {
                            pos -= c.len_utf8();
                        } else {
                            break;
                        }
                    }
                } else {
                    // We're after a word character - check if we're at the start of a word
                    // by looking at the character before that
                    let two_back = if pos >= c.len_utf8() {
                        input[..pos - c.len_utf8()].chars().next_back()
                    } else {
                        None
                    };

                    if two_back.is_none() || two_back.is_some_and(|c2| c2.is_whitespace()) {
                        // At start of word - move to previous word
                        pos -= c.len_utf8(); // Move past the first char of current word
                        // Skip whitespace backwards
                        while pos > 0 {
                            if let Some(c) = input[..pos].chars().next_back()
                                && c.is_whitespace()
                            {
                                pos -= c.len_utf8();
                            } else {
                                break;
                            }
                        }
                        // Skip the previous word backwards
                        while pos > 0 {
                            if let Some(c) = input[..pos].chars().next_back()
                                && !c.is_whitespace()
                            {
                                pos -= c.len_utf8();
                            } else {
                                break;
                            }
                        }
                    } else {
                        // In middle of word - go to start of current word
                        while pos > 0 {
                            if let Some(c) = input[..pos].chars().next_back()
                                && !c.is_whitespace()
                            {
                                pos -= c.len_utf8();
                            } else {
                                break;
                            }
                        }
                    }
                }
            }
            pos
        }
        VimMotion::_Line => len, // Special case, usually handled by operator logic
        VimMotion::_CharRight => start + input[start..].chars().next().map_or(0, |c| c.len_utf8()),
        VimMotion::_CharLeft => {
            start
                - input[..start]
                    .chars()
                    .next_back()
                    .map_or(0, |c| c.len_utf8())
        }
        VimMotion::_StartOfLine => 0,
        VimMotion::_EndOfLine => len,
    };

    (start, end)
}

fn execute_operator(state: &mut MutexGuard<'_, App>, operator: VimOperator, range: (usize, usize)) {
    let (start, end) = range;
    let (low, high) = if start < end {
        (start, end)
    } else {
        (end, start)
    };

    match operator {
        VimOperator::Delete => {
            if high > low
                && state.data.input.buffer.is_char_boundary(low)
                && state.data.input.buffer.is_char_boundary(high)
            {
                let deleted = state.data.input.buffer[low..high].to_string();
                if let Some(vim_state) = &mut state.data.vim.state {
                    vim_state.yank_buffer = deleted;
                }
                state.data.input.buffer.drain(low..high);
                state.data.cursor_position = low;
            }
        }
        VimOperator::_Change => {
            // Not implemented yet
        }
        VimOperator::_Yank => {
            // Not implemented yet
        }
        VimOperator::Goto => {
            todo!();
        }
    }
}

pub async fn handle_vim_keys(
    mut state: MutexGuard<'_, App>,
    c: char,
    tx_action: Sender<AppAction>,
) {
    // Check for timeout
    if let Some(vim_state) = &mut state.data.vim.state
        && vim_state.operator.is_some()
        && Instant::now()
            .duration_since(vim_state.last_action_time)
            .as_secs()
            >= 1
    {
        vim_state.operator = None;
        vim_state.pending_keys.clear();
    }

    // Ensure vim_state exists (it should, but for safety)
    if state.data.vim.state.is_none() {
        state.data.vim.state = Some(VimState::default());
    }

    // We need to clone some state to avoid borrow checker issues when calling async functions
    // or when mutating state later.
    let current_operator = state.data.vim.state.as_ref().unwrap().operator;

    if let AppState::Chatting(channel) = &state.state
        && state.data.selection_index > 0
        && ['i', 'I', 'a', 'A'].contains(&c)
    {
        let msg_index_in_slice = state.data.selection_index.saturating_sub(1);

        if let Some(msg) = state.data.guilds.messages.get(msg_index_in_slice)
            && state
                .data
                .current_user
                .as_ref()
                .is_some_and(|user| user.id == msg.author.id)
        {
            tx_action
                .send(AppAction::TransitionToEditing(
                    channel.clone(),
                    msg.clone(),
                    msg.content.clone().unwrap_or_default(),
                    c,
                ))
                .await
                .ok();
        }
        return;
    }

    match c {
        'i' => {
            state.data.input.mode = InputMode::Insert;
        }
        'I' => {
            let start_of_line = state.data.input.buffer[..state.data.cursor_position]
                .rfind('\n')
                .map(|i| i + 1)
                .unwrap_or(0);
            state.data.cursor_position = start_of_line;
            state.data.input.mode = InputMode::Insert;
        }
        'a' => {
            if let Some(c) = state.data.input.buffer[state.data.cursor_position..]
                .chars()
                .next()
            {
                state.data.cursor_position += c.len_utf8();
            }
            state.data.input.mode = InputMode::Insert;
        }
        'A' => {
            let end_of_line = state.data.input.buffer[state.data.cursor_position..]
                .find('\n')
                .map(|i| state.data.cursor_position + i)
                .unwrap_or(state.data.input.buffer.len());
            state.data.cursor_position = end_of_line;
            state.data.input.mode = InputMode::Insert;
        }
        'O' => {
            if let AppState::Chatting(_) = &state.state
                && state.data.selection_index > 0
            {
                return;
            }
            let current_line_start = state.data.input.buffer[..state.data.cursor_position]
                .rfind('\n')
                .map(|i| i + 1)
                .unwrap_or(0);
            state.data.input.buffer.insert(current_line_start, '\n');
            state.data.cursor_position = current_line_start;
            state.data.input.mode = InputMode::Insert;
        }
        'o' => {
            if let AppState::Chatting(_) = &state.state
                && state.data.selection_index > 0
            {
                return;
            }
            let next_line_start = state.data.input.buffer[state.data.cursor_position..]
                .find('\n')
                .map(|i| state.data.cursor_position + i + 1)
                .unwrap_or(state.data.input.buffer.len());

            if next_line_start < state.data.input.buffer.len() {
                state.data.input.buffer.insert(next_line_start, '\n');
                state.data.cursor_position = next_line_start;
            } else {
                state.data.input.buffer.push('\n');
                state.data.cursor_position = next_line_start + 1;
            }

            state.data.input.mode = InputMode::Insert;
        }
        'j' => {
            if let AppState::Chatting(_) | AppState::Logs(_) = &state.state {
                if state.data.selection_index > 0 {
                    state.data.selection_index -= 1;
                } else {
                    let current_pos = state.data.cursor_position;
                    let current_line_start = state.data.input.buffer[..current_pos]
                        .rfind('\n')
                        .map(|i| i + 1)
                        .unwrap_or(0);
                    let current_column_width = UnicodeWidthStr::width(
                        &state.data.input.buffer[current_line_start..current_pos],
                    );

                    if let Some(newline_offset) = state.data.input.buffer[current_pos..].find('\n')
                    {
                        let next_line_start = current_pos + newline_offset + 1;
                        if next_line_start < state.data.input.buffer.len() {
                            let next_line_end = state.data.input.buffer[next_line_start..]
                                .find('\n')
                                .map(|i| next_line_start + i)
                                .unwrap_or(state.data.input.buffer.len());
                            let next_line_str =
                                &state.data.input.buffer[next_line_start..next_line_end];

                            let mut target_offset = 0;
                            let mut current_width = 0;
                            for c in next_line_str.chars() {
                                let w = c.width().unwrap_or(0); // Optimization: avoid c.to_string() allocation
                                if current_width + w > current_column_width {
                                    break;
                                }
                                current_width += w;
                                target_offset += c.len_utf8();
                            }
                            if target_offset == next_line_str.len()
                                && target_offset > 0
                                && let Some(last_char) = next_line_str.chars().next_back()
                            {
                                target_offset -= last_char.len_utf8();
                            }
                            state.data.cursor_position = next_line_start + target_offset;
                            clamp_cursor(&mut state);
                        }
                    }
                }
            } else {
                tx_action.send(AppAction::SelectNext).await.ok();
            }
        }
        'k' => {
            if let AppState::Chatting(channel) = &state.state {
                if state.data.selection_index != 0 {
                    if state.data.selection_index < state.data.guilds.messages.len() {
                        state.data.selection_index += 1;
                    } else if !state.is_loading {
                        tx_action
                            .send(AppAction::TransitionToLoadingMessages)
                            .await
                            .ok();

                        if let Some(oldest) = state.data.guilds.messages.last() {
                            let older_msgs = state
                                .client
                                .api
                                .get_channel_messages(
                                    &channel.get_id().clone(),
                                    None,
                                    Some(oldest.id.clone()),
                                    None,
                                    Some(100),
                                )
                                .await;

                            if let Ok(new_messages) = older_msgs {
                                for msg in new_messages.into_iter() {
                                    state.data.guilds.messages.push(msg);
                                }
                            }
                        }

                        tx_action.send(AppAction::EndLoadingMessages).await.ok();
                    }
                } else {
                    let current_pos = state.data.cursor_position;
                    let current_column_width = {
                        let current_line_start = state.data.input.buffer[..current_pos]
                            .rfind('\n')
                            .map(|i| i + 1)
                            .unwrap_or(0);
                        UnicodeWidthStr::width(
                            &state.data.input.buffer[current_line_start..current_pos],
                        )
                    };

                    let input_before = &state.data.input.buffer[..current_pos];

                    if let Some(last_newline) = input_before.rfind('\n') {
                        let prev_line_start = state.data.input.buffer[..last_newline]
                            .rfind('\n')
                            .map(|i| i + 1)
                            .unwrap_or(0);
                        let prev_line_end = last_newline;
                        let prev_line_str =
                            &state.data.input.buffer[prev_line_start..prev_line_end];

                        let mut target_offset = 0;
                        let mut current_width = 0;
                        for c in prev_line_str.chars() {
                            let w = c.width().unwrap_or(0); // Optimization: avoid c.to_string() allocation
                            if current_width + w > current_column_width {
                                break;
                            }
                            current_width += w;
                            target_offset += c.len_utf8();
                        }
                        if target_offset == prev_line_str.len()
                            && target_offset > 0
                            && let Some(last_char) = prev_line_str.chars().next_back()
                        {
                            target_offset -= last_char.len_utf8();
                        }
                        state.data.cursor_position = prev_line_start + target_offset;
                        clamp_cursor(&mut state);
                    } else if !state.data.guilds.messages.is_empty() {
                        state.data.selection_index = 1;
                    }
                }
            } else if let AppState::Logs(_) = &state.state {
                if state.data.selection_index != 0 {
                    if state.data.selection_index < state.data.logs.len() {
                        state.data.selection_index += 1;
                    } else {
                        match state.data.log_reader.read_previous_lines(10).await {
                            Ok(old_logs) => {
                                if !old_logs.is_empty() {
                                    for log in old_logs {
                                        state.data.logs.push(log);
                                    }
                                }
                            }
                            Err(e) => {
                                print_log(
                                    format!("Failed to read previous logs: {e}").into(),
                                    LogType::Error,
                                )
                                .await
                                .ok();
                            }
                        }
                    }
                } else {
                    let current_pos = state.data.cursor_position;
                    let current_column_width = {
                        let current_line_start = state.data.input.buffer[..current_pos]
                            .rfind('\n')
                            .map(|i| i + 1)
                            .unwrap_or(0);
                        UnicodeWidthStr::width(
                            &state.data.input.buffer[current_line_start..current_pos],
                        )
                    };

                    let input_before = &state.data.input.buffer[..current_pos];

                    if let Some(last_newline) = input_before.rfind('\n') {
                        let prev_line_start = state.data.input.buffer[..last_newline]
                            .rfind('\n')
                            .map(|i| i + 1)
                            .unwrap_or(0);
                        let prev_line_end = last_newline;
                        let prev_line_str =
                            &state.data.input.buffer[prev_line_start..prev_line_end];

                        let mut target_offset = 0;
                        let mut current_width = 0;
                        for c in prev_line_str.chars() {
                            let w = c.width().unwrap_or(0); // Optimization: avoid c.to_string() allocation
                            if current_width + w > current_column_width {
                                break;
                            }
                            current_width += w;
                            target_offset += c.len_utf8();
                        }
                        if target_offset == prev_line_str.len()
                            && target_offset > 0
                            && let Some(last_char) = prev_line_str.chars().next_back()
                        {
                            target_offset -= last_char.len_utf8();
                        }
                        state.data.cursor_position = prev_line_start + target_offset;
                        clamp_cursor(&mut state);
                    } else if !state.data.logs.is_empty() {
                        state.data.selection_index = 1;
                    }
                }
            } else {
                tx_action.send(AppAction::SelectPrevious).await.ok();
            }
        }
        'h' => {
            if let Some(c) = state.data.input.buffer[..state.data.cursor_position]
                .chars()
                .next_back()
                && (!c.is_control() || c == '\t')
            {
                state.data.cursor_position -= c.len_utf8();
            }
        }
        'l' => {
            if let Some(c) = state.data.input.buffer[state.data.cursor_position..]
                .chars()
                .next()
                && c != '\n'
            {
                let next_pos = state.data.cursor_position + c.len_utf8();
                // Optional: check if next_pos lands on newline and decide whether to step onto it?
                // For now, simply blocking movement FROM newline (checked above) prevents wrapping to next line.
                // But we also want to maybe stop AT the last char, not ON the newline.
                // If we want to emulate vim standard behavior:
                // If next char is '\n', we DON'T move onto it?
                if let Some(next_c) = state.data.input.buffer[next_pos..].chars().next() {
                    if next_c != '\n' {
                        state.data.cursor_position = next_pos;
                    }
                } else if next_pos < state.data.input.buffer.len() {
                    // End of file case
                    state.data.cursor_position = next_pos;
                }
            }
        }
        'w' => {
            if let AppState::Chatting(_) = &state.state
                && state.data.selection_index > 0
            {
                return;
            }
            if let Some(op) = current_operator {
                let range = get_motion_range(&state, VimMotion::WordForward);
                execute_operator(&mut state, op, range);
                if let Some(vim_state) = &mut state.data.vim.state {
                    vim_state.operator = None;
                }
            } else {
                let (_, end) = get_motion_range(&state, VimMotion::WordForward);
                state.data.cursor_position = end;
                clamp_cursor(&mut state);
            }
        }
        'b' => {
            if let AppState::Chatting(_) = &state.state
                && state.data.selection_index > 0
            {
                return;
            }
            if let Some(op) = current_operator {
                let range = get_motion_range(&state, VimMotion::WordBackward);
                execute_operator(&mut state, op, range);
                if let Some(vim_state) = &mut state.data.vim.state {
                    vim_state.operator = None;
                }
            } else {
                let (_, end) = get_motion_range(&state, VimMotion::WordBackward);
                state.data.cursor_position = end;
            }
        }
        'd' => {
            if state.data.input.mode == InputMode::Visual
                || state.data.input.mode == InputMode::VisualLine
            {
                let visual_start = state.data.vim.state.as_ref().and_then(|vs| vs.visual_start);
                if let Some(vs) = visual_start {
                    let mut start = vs.min(state.data.cursor_position);
                    let mut end = vs.max(state.data.cursor_position);
                    let end_len = state.data.input.buffer[end..]
                        .chars()
                        .next()
                        .map(|c| c.len_utf8())
                        .unwrap_or(0);
                    end = (end + end_len).min(state.data.input.buffer.len());
                    if state.data.input.mode == InputMode::VisualLine {
                        start = state.data.input.buffer[..start]
                            .rfind('\n')
                            .map(|i| i + 1)
                            .unwrap_or(0);
                        end = state.data.input.buffer[end..]
                            .find('\n')
                            .map(|i| end + i + 1)
                            .unwrap_or(state.data.input.buffer.len());
                    }
                    if start < end {
                        let deleted: String = state.data.input.buffer.drain(start..end).collect();
                        let is_linewise = state.data.input.mode == InputMode::VisualLine;
                        if let Some(vim_state) = &mut state.data.vim.state {
                            if is_linewise {
                                vim_state.yank_buffer = format!("\n{}", deleted.trim_matches('\n'));
                            } else {
                                vim_state.yank_buffer = deleted;
                            }
                        }
                        state.data.cursor_position = start;
                    }
                }
                if let Some(vim_state) = &mut state.data.vim.state {
                    vim_state.visual_start = None;
                }
                state.data.input.mode = InputMode::Normal;
                clamp_cursor(&mut state);
                return;
            }

            if let AppState::Chatting(channel) = &state.state
                && state.data.selection_index > 0
            {
                if let Some(VimOperator::Delete) = current_operator {
                    // User pressed dd on a historical message!
                    let msg_index_in_slice = state.data.selection_index.saturating_sub(1);

                    if let Some(msg) = state.data.guilds.messages.get(msg_index_in_slice) {
                        // Check if the current user is the author
                        if state
                            .data
                            .current_user
                            .as_ref()
                            .is_some_and(|user| user.id == msg.author.id)
                        {
                            let msg_id = msg.id.clone();
                            let ch_id = channel.get_id().clone();

                            tx_action
                                .send(AppAction::ApiDeleteMessage(ch_id, msg_id))
                                .await
                                .ok();

                            // Reset the operator immediately to not trigger regular deletion on selection change later
                            if let Some(vim_state) = &mut state.data.vim.state {
                                vim_state.operator = None;
                            }
                        }
                        // If they are not the author, do nothing (we could flash status natively).
                    }
                } else if let Some(vim_state) = &mut state.data.vim.state {
                    vim_state.operator = Some(VimOperator::Delete);
                    vim_state.last_action_time = Instant::now();
                }
                return;
            }
            if let Some(VimOperator::Delete) = current_operator {
                let current_pos = state.data.cursor_position;
                let current_line_start = state.data.input.buffer[..current_pos]
                    .rfind('\n')
                    .map(|i| i + 1)
                    .unwrap_or(0);

                if let Some(newline_offset) = state.data.input.buffer[current_pos..].find('\n') {
                    let next_newline_index = current_pos + newline_offset;
                    let deleted: String = state
                        .data
                        .input
                        .buffer
                        .drain(current_line_start..next_newline_index + 1)
                        .collect();
                    if let Some(vim_state) = &mut state.data.vim.state {
                        vim_state.yank_buffer = format!("\n{}", deleted.trim_matches('\n'));
                    }
                    state.data.cursor_position = current_line_start;
                } else if current_line_start > 0 {
                    let len = state.data.input.buffer.len();
                    let deleted: String = state
                        .data
                        .input
                        .buffer
                        .drain(current_line_start - 1..len)
                        .collect();
                    if let Some(vim_state) = &mut state.data.vim.state {
                        vim_state.yank_buffer = format!("\n{}", deleted.trim_matches('\n'));
                    }
                    let prev_line_start = state.data.input.buffer[..current_line_start - 1]
                        .rfind('\n')
                        .map(|i| i + 1)
                        .unwrap_or(0);
                    state.data.cursor_position = prev_line_start;
                } else {
                    let deleted = state.data.input.buffer.clone();
                    state.data.input.buffer.clear();
                    if let Some(vim_state) = &mut state.data.vim.state {
                        vim_state.yank_buffer = format!("\n{}", deleted.trim_matches('\n'));
                    }
                    state.data.cursor_position = 0;
                }

                clamp_cursor(&mut state);

                if let Some(vim_state) = &mut state.data.vim.state {
                    vim_state.operator = None;
                }
            } else if let Some(vim_state) = &mut state.data.vim.state {
                vim_state.operator = Some(VimOperator::Delete);
                vim_state.last_action_time = Instant::now();
            }
        }

        'v' => {
            if state.data.input.mode == InputMode::Normal {
                state.data.input.mode = InputMode::Visual;
                let cp = state.data.cursor_position;
                if let Some(vim_state) = &mut state.data.vim.state {
                    vim_state.visual_start = Some(cp);
                }
            } else if state.data.input.mode == InputMode::Visual {
                state.data.input.mode = InputMode::Normal;
                if let Some(vim_state) = &mut state.data.vim.state {
                    vim_state.visual_start = None;
                }
            }
        }
        'V' => {
            if state.data.input.mode == InputMode::Normal {
                state.data.input.mode = InputMode::VisualLine;
                let cp = state.data.cursor_position;
                if let Some(vim_state) = &mut state.data.vim.state {
                    vim_state.visual_start = Some(cp);
                }
            } else if state.data.input.mode == InputMode::VisualLine {
                state.data.input.mode = InputMode::Normal;
                if let Some(vim_state) = &mut state.data.vim.state {
                    vim_state.visual_start = None;
                }
            }
        }
        'x' => {
            if state.data.input.mode == InputMode::Visual
                || state.data.input.mode == InputMode::VisualLine
            {
                let visual_start = state.data.vim.state.as_ref().and_then(|vs| vs.visual_start);
                if let Some(vs) = visual_start {
                    let mut start = vs.min(state.data.cursor_position);
                    let mut end = vs.max(state.data.cursor_position);
                    let end_len = state.data.input.buffer[end..]
                        .chars()
                        .next()
                        .map(|c| c.len_utf8())
                        .unwrap_or(0);
                    end = (end + end_len).min(state.data.input.buffer.len());
                    if state.data.input.mode == InputMode::VisualLine {
                        start = state.data.input.buffer[..start]
                            .rfind('\n')
                            .map(|i| i + 1)
                            .unwrap_or(0);
                        end = state.data.input.buffer[end..]
                            .find('\n')
                            .map(|i| end + i + 1)
                            .unwrap_or(state.data.input.buffer.len());
                    }
                    if start < end {
                        let deleted: String = state.data.input.buffer.drain(start..end).collect();
                        let is_linewise = state.data.input.mode == InputMode::VisualLine;
                        if let Some(vim_state) = &mut state.data.vim.state {
                            if is_linewise {
                                vim_state.yank_buffer = format!("\n{}", deleted.trim_matches('\n'));
                            } else {
                                vim_state.yank_buffer = deleted;
                            }
                        }
                        state.data.cursor_position = start;
                    }
                }
                if let Some(vim_state) = &mut state.data.vim.state {
                    vim_state.visual_start = None;
                }
                state.data.input.mode = InputMode::Normal;
                clamp_cursor(&mut state);
                return;
            }

            if let AppState::Chatting(_) = &state.state
                && state.data.selection_index > 0
            {
                return;
            }
            let pos = state.data.cursor_position;
            if pos < state.data.input.buffer.len()
                && state.data.input.buffer.is_char_boundary(pos)
                && let Some(ch) = state.data.input.buffer[pos..].chars().next()
            {
                let char_end = pos + ch.len_utf8();
                let deleted: String = state.data.input.buffer.drain(pos..char_end).collect();
                if let Some(vim_state) = &mut state.data.vim.state {
                    vim_state.yank_buffer = deleted;
                }
                clamp_cursor(&mut state);
            }
        }
        'y' => {
            if state.data.input.mode == InputMode::Visual
                || state.data.input.mode == InputMode::VisualLine
            {
                let visual_start = state.data.vim.state.as_ref().and_then(|vs| vs.visual_start);
                if let Some(vs) = visual_start {
                    let mut start = vs.min(state.data.cursor_position);
                    let mut end = vs.max(state.data.cursor_position);
                    let end_len = state.data.input.buffer[end..]
                        .chars()
                        .next()
                        .map(|c| c.len_utf8())
                        .unwrap_or(0);
                    end = (end + end_len).min(state.data.input.buffer.len());
                    if state.data.input.mode == InputMode::VisualLine {
                        start = state.data.input.buffer[..start]
                            .rfind('\n')
                            .map(|i| i + 1)
                            .unwrap_or(0);
                        end = state.data.input.buffer[end..]
                            .find('\n')
                            .map(|i| end + i + 1)
                            .unwrap_or(state.data.input.buffer.len());
                    }
                    if start < end {
                        let yanked = state.data.input.buffer[start..end].to_string();
                        let is_linewise = state.data.input.mode == InputMode::VisualLine;
                        if let Some(vim_state) = &mut state.data.vim.state {
                            if is_linewise {
                                vim_state.yank_buffer = format!("\n{}", yanked.trim_matches('\n'));
                            } else {
                                vim_state.yank_buffer = yanked;
                            }
                        }
                    }
                }
                if let Some(vim_state) = &mut state.data.vim.state {
                    vim_state.visual_start = None;
                }
                state.data.input.mode = InputMode::Normal;
                clamp_cursor(&mut state);
            } else {
                if let Some(VimOperator::_Yank) = current_operator {
                    let current_pos = state.data.cursor_position;
                    let current_line_start = state.data.input.buffer[..current_pos]
                        .rfind('\n')
                        .map(|i| i + 1)
                        .unwrap_or(0);

                    let next_newline_index = state.data.input.buffer[current_pos..]
                        .find('\n')
                        .map(|i| current_pos + i)
                        .unwrap_or(state.data.input.buffer.len());

                    let yanked =
                        state.data.input.buffer[current_line_start..next_newline_index].to_string();
                    if let Some(vim_state) = &mut state.data.vim.state {
                        vim_state.yank_buffer = format!("\n{}", yanked.trim_matches('\n'));
                        vim_state.operator = None;
                    }
                } else if let Some(vim_state) = &mut state.data.vim.state {
                    vim_state.operator = Some(VimOperator::_Yank);
                    vim_state.last_action_time = Instant::now();
                }
            }
        }
        'p' => {
            if let AppState::Chatting(_) = &state.state
                && state.data.selection_index > 0
            {
                return;
            }
            if let Some(vim_state) = &state.data.vim.state {
                let yanked = vim_state.yank_buffer.clone();
                if !yanked.is_empty() {
                    let mut pos = state.data.cursor_position;
                    if yanked.starts_with('\n') {
                        pos = state.data.input.buffer[pos..]
                            .find('\n')
                            .map(|i| pos + i)
                            .unwrap_or(state.data.input.buffer.len());
                        state.data.input.buffer.insert_str(pos, &yanked);
                        state.data.cursor_position = pos + 1;
                    } else {
                        if pos < state.data.input.buffer.len() {
                            let char_len = state.data.input.buffer[pos..]
                                .chars()
                                .next()
                                .map(|c| c.len_utf8())
                                .unwrap_or(0);
                            pos += char_len;
                        }
                        state.data.input.buffer.insert_str(pos, &yanked);
                        let last_char_len = yanked
                            .chars()
                            .next_back()
                            .map(|c| c.len_utf8())
                            .unwrap_or(0);
                        state.data.cursor_position = pos + yanked.len() - last_char_len;
                    }
                    clamp_cursor(&mut state);
                }
            }
        }
        'P' => {
            if let AppState::Chatting(_) = &state.state
                && state.data.selection_index > 0
            {
                return;
            }
            if let Some(vim_state) = &state.data.vim.state {
                let yanked = vim_state.yank_buffer.clone();
                if !yanked.is_empty() {
                    let mut pos = state.data.cursor_position;
                    if let Some(stripped) = yanked.strip_prefix('\n') {
                        pos = state.data.input.buffer[..pos]
                            .rfind('\n')
                            .map(|i| i + 1)
                            .unwrap_or(0);
                        let to_insert = format!("{}\n", stripped);
                        state.data.input.buffer.insert_str(pos, &to_insert);
                        state.data.cursor_position = pos;
                    } else {
                        state.data.input.buffer.insert_str(pos, &yanked);
                        let last_char_len = yanked
                            .chars()
                            .next_back()
                            .map(|c| c.len_utf8())
                            .unwrap_or(0);
                        state.data.cursor_position = pos + yanked.len() - last_char_len;
                    }
                    clamp_cursor(&mut state);
                }
            }
        }
        'g' => {
            if let Some(VimOperator::Goto) = current_operator {
                match &state.state {
                    AppState::Chatting(_) => {
                        state.data.selection_index = state.data.guilds.messages.len();
                    }
                    AppState::Logs(_) => {
                        state.data.selection_index = state.data.logs.len();
                    }
                    _ => {
                        state.data.selection_index = 0;
                    }
                }
            } else if let Some(vim_state) = &mut state.data.vim.state {
                vim_state.operator = Some(VimOperator::Goto);
                vim_state.last_action_time = Instant::now();
            }
        }
        'G' => match &state.state {
            AppState::Home => {
                state.data.selection_index = 2;
            }
            AppState::SelectingGuild => {
                state.data.selection_index = state
                    .data
                    .guilds
                    .joined
                    .iter()
                    .filter(|g| {
                        g.name
                            .to_lowercase()
                            .contains(state.data.input.search.to_lowercase().as_str())
                    })
                    .collect::<Vec<&PartialGuild>>()
                    .len()
                    .saturating_sub(1);
            }
            AppState::SelectingDM => {
                state.data.selection_index = state
                    .data
                    .dms
                    .channels
                    .iter()
                    .filter(|dm| {
                        dm.get_name()
                            .to_lowercase()
                            .contains(state.data.input.search.to_lowercase().as_str())
                    })
                    .collect::<Vec<&DM>>()
                    .len()
                    .saturating_sub(1);
            }
            AppState::SelectingChannel(_) => {
                let filter_text = state.data.input.search.to_lowercase();
                let permission_context = &state.data.context;
                let mut list_items: Vec<&Channel> = Vec::new();
                let should_display_channel_content = |c: &Channel| {
                    let is_readable = permission_context
                        .as_ref()
                        .is_some_and(|context| c.is_readable(context));

                    is_readable
                        && (filter_text.is_empty() || c.name.to_lowercase().contains(&filter_text))
                };

                state
                    .data
                    .guilds
                    .channels
                    .iter()
                    .filter(|c| {
                        if c.children.is_none() && c.channel_type != 4 {
                            return should_display_channel_content(c);
                        }
                        if c.channel_type == 4 {
                            if filter_text.is_empty()
                                || c.name.to_lowercase().contains(&filter_text)
                            {
                                return true;
                            }
                            if let Some(children) = &c.children {
                                return children.iter().any(should_display_channel_content);
                            }
                        }
                        false
                    })
                    .for_each(|c| {
                        if c.channel_type == 4 {
                            list_items.push(c);
                            if let Some(children) = &c.children {
                                children
                                    .iter()
                                    .filter(|c| should_display_channel_content(c))
                                    .for_each(|child| list_items.push(child));
                            }
                        } else {
                            list_items.push(c);
                        }
                    });

                state.data.selection_index = list_items.len().saturating_sub(1);
            }
            AppState::Chatting(_) | AppState::Logs(_) => {
                state.data.selection_index = 0;

                let len = state.data.input.buffer.len();
                state.data.cursor_position = len;
                clamp_cursor(&mut state);
            }
            _ => {}
        },
        ':' => {
            state.data.input.saved = Some(state.data.input.buffer.clone());
            state.data.input.buffer.clear();
            state.data.cursor_position = 0;
            state.data.input.mode = InputMode::Command;
        }
        '/' => {
            state.data.input.saved = Some(state.data.input.buffer.clone());
            state.data.input.buffer.clear();
            state.data.cursor_position = 0;
            state.data.input.mode = InputMode::Search;
        }
        _ => {
            if let Some(vim_state) = &mut state.data.vim.state {
                vim_state.operator = None;
                vim_state.pending_keys.clear();
            }
        }
    }
}
