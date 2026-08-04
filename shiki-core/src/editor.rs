//! Detects the user's OS-level "favorite" text editor, for the
//! `use_favorite_editor` config option — an alternative to always opening
//! the built-in inline editor or a hardcoded `$EDITOR`.

// Only used by `desktop_exec_command` below, which is Linux-only — a
// pre-existing unconditional import here was unused (and thus a clippy
// error under `-D warnings`) on every other target; caught once CI's
// `fmt-and-clippy` job actually started running on macOS/Windows too,
// instead of only ubuntu-latest.
#[cfg(target_os = "linux")]
use std::path::PathBuf;

/// Resolves the editor to launch, in priority order:
/// `$VISUAL` -> `$EDITOR` -> the OS's registered default text editor ->
/// `None` (caller decides the final fallback, e.g. the configured editor).
pub fn detect_favorite_editor() -> Option<String> {
    for var in ["VISUAL", "EDITOR"] {
        if let Ok(value) = std::env::var(var) {
            if !value.trim().is_empty() {
                return Some(value);
            }
        }
    }

    #[cfg(target_os = "linux")]
    if let Some(editor) = linux_default_editor() {
        return Some(editor);
    }

    #[cfg(target_os = "macos")]
    if let Some(editor) = macos_default_editor() {
        return Some(editor);
    }

    None
}

/// Asks the desktop's MIME database what opens `text/plain` (respected by
/// GNOME, KDE, and every other freedesktop-compliant environment), then
/// reads that `.desktop` file's `Exec=` line for the actual command.
#[cfg(target_os = "linux")]
fn linux_default_editor() -> Option<String> {
    let output = std::process::Command::new("xdg-mime")
        .args(["query", "default", "text/plain"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let desktop_file = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if desktop_file.is_empty() {
        return None;
    }
    desktop_exec_command(&desktop_file)
}

#[cfg(target_os = "linux")]
fn desktop_exec_command(desktop_file: &str) -> Option<String> {
    let mut dirs = Vec::new();
    if let Ok(xdg_data_home) = std::env::var("XDG_DATA_HOME") {
        dirs.push(PathBuf::from(xdg_data_home).join("applications"));
    } else if let Ok(home) = std::env::var("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/applications"));
    }
    dirs.push(PathBuf::from("/usr/local/share/applications"));
    dirs.push(PathBuf::from("/usr/share/applications"));

    for dir in dirs {
        let contents = match std::fs::read_to_string(dir.join(desktop_file)) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let exec_line = contents.lines().find_map(|l| l.strip_prefix("Exec="))?;
        // Desktop field codes (%f, %F, %u, %U, ...) are separate whitespace
        // tokens — taking just the first token gives the bare command.
        let command = exec_line.split_whitespace().next()?;
        return Some(command.to_string());
    }
    None
}

/// macOS has no single CLI equivalent to `xdg-mime` without extra tooling
/// (e.g. `duti`); `open -W -t` reliably opens (and waits on) the user's
/// default text editor GUI app, so we shell out to that instead of trying
/// to resolve a binary.
#[cfg(target_os = "macos")]
fn macos_default_editor() -> Option<String> {
    Some("open -W -t".to_string())
}

/// Splits an editor command string into tokens, honoring `"..."`/`'...'`
/// quoting around a single token — needed for a program path that itself
/// contains a space (common on Windows, e.g. `"C:\Program Files\Editor\
/// editor.exe" --wait`), which a plain `split_whitespace` would otherwise
/// tear into bogus argv pieces. Deliberately minimal: no escape sequences,
/// no nested quotes — just enough to keep one quoted path together.
fn split_command(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes: Option<char> = None;
    for c in input.chars() {
        match in_quotes {
            Some(q) if c == q => in_quotes = None,
            Some(_) => current.push(c),
            None if c == '"' || c == '\'' => in_quotes = Some(c),
            None if c.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            None => current.push(c),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Splits an editor command string (e.g. `"code --wait"` or `"open -W -t"`)
/// into a program + base args, then builds a ready-to-run `Command` with
/// `path` appended as the final argument. Editor strings are single words
/// in the common case (`"nvim"`), but favorite-editor detection and some
/// user configs produce multi-word commands, so callers should always go
/// through this instead of `Command::new(editor)` directly.
pub fn command_for(editor: &str, path: &std::path::Path) -> std::process::Command {
    let mut parts = split_command(editor);
    if parts.is_empty() {
        parts.push(editor.to_string());
    }
    let program = parts.remove(0);
    let mut command = std::process::Command::new(program);
    command.args(parts);
    command.arg(path);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_command_splits_plain_whitespace() {
        assert_eq!(split_command("code --wait"), vec!["code", "--wait"]);
    }

    #[test]
    fn split_command_keeps_a_quoted_path_with_spaces_together() {
        assert_eq!(
            split_command(r#""C:\Program Files\Editor\editor.exe" --wait"#),
            vec![r"C:\Program Files\Editor\editor.exe", "--wait"]
        );
    }

    #[test]
    fn split_command_supports_single_quotes_too() {
        assert_eq!(
            split_command("'my editor' --flag"),
            vec!["my editor", "--flag"]
        );
    }

    #[test]
    fn command_for_uses_quoted_program_as_a_single_argv0() {
        let command = command_for(
            r#""C:\Program Files\Editor\editor.exe" --wait"#,
            std::path::Path::new("note.md"),
        );
        assert_eq!(
            command.get_program().to_string_lossy(),
            r"C:\Program Files\Editor\editor.exe"
        );
    }
}
