# Keybinds

## Global Keybinds

| Keybind | Action |
|---------|--------|
| `Ctrl + C` | Quit the application |
| `Esc` | Go back to previous menu, cancel selection, or return to Normal mode |
| `Enter` | Submit message or select an item |
| `Up` / `Down` | Navigate lists of items (guilds, channels, DMs) |
| `Left` / `Right` | Move text cursor |
| `Backspace` / `Delete` | Delete characters (Insert mode); in Vim Normal mode, Backspace moves cursor left |

## Vim Mode Keybinds

When Vim mode is enabled, typing text is done in **Insert Mode** while navigation and manipulation are done in **Normal Mode**. 

### Entering Insert Mode (from Normal Mode)

| Keybind | Action |
|---------|--------|
| `i` | Insert at current cursor position |
| `I` | Insert at the beginning of the line |
| `a` | Append after the cursor |
| `A` | Append at the end of the line |
| `o` | Insert a new line below and enter Insert mode |
| `O` | Insert a new line above and enter Insert mode |

### Navigation & Editing (Normal Mode)

| Keybind | Action |
|---------|--------|
| `h` | Move cursor left |
| `l` | Move cursor right |
| `j` | Move cursor down (or select next message in chat) |
| `k` | Move cursor up (or select previous message in chat) |
| `w` | Move cursor forward to the next word |
| `b` | Move cursor backward to the previous word |
| `v` | Enter Visual mode for text selection |
| `V` | Enter Visual Line mode for selecting entire lines |
| `y` | Yank (copy) the current selection; `yy` yanks current line |
| `p` / `P` | Paste the yanked text (after/before cursor, or line-wise) |
| `x` | Delete the character under the cursor (or selection in Visual mode) |
| `dw` | Delete the word in front of the cursor |
| `db` | Delete the word before the cursor |
| `dd` | Delete the current line (or delete the selected message if you authored it) |
| `d` | Delete the selected text (in Visual mode) |
| `gg` | Move cursor to the start of selection |
| `G` | Move cursor to the end of input (or clear message selection) |

### Command & Search

For a full list of available Vim-style commands, please refer to the [Commands Documentation](commands.md).

| Keybind | Action |
|---------|--------|
| `:` | Enter Command Mode (e.g., `:quit`, `:debug`) |
| `/` | Enter Search Mode (global filter for channels, DMs, guilds, and messages) |
