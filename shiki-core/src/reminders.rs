//! Opt-in background task-due reminders (`general.enable_reminders`) — a
//! thread that periodically re-scans every notebook for checkbox tasks
//! (`tasks::extract`) due today or overdue and fires a real OS desktop
//! notification for each, once per calendar day. Deliberately fire-and-
//! forget: unlike `shiki-tui`'s capture daemon, nothing here needs to push
//! a result back into `App` for rendering, so the whole thing — scan,
//! dedup, notify, background thread — lives in this crate and is spawned
//! identically by the TUI (`App::new`) and headless `shiki daemon`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::{tasks, Note, Notebook, NotebookStore, Result};

/// One task that's newly due — pure output of `scan_due_reminders`, no I/O.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueReminder {
    pub notebook: String,
    pub note_title: String,
    pub task_text: String,
    pub due: NaiveDate,
    pub overdue: bool,
    /// Stable dedup identity: note path + occurrence + exact line — the
    /// same identity `tasks::toggle` already addresses a task by, so it
    /// stays valid as long as the task's line itself doesn't change.
    pub key: String,
}

fn reminder_key(note: &Note, occurrence: usize, raw_line: &str) -> String {
    format!("{}|{occurrence}|{raw_line}", note.path.display())
}

/// Every pending task due today or overdue across `pool`, excluding
/// anything already in `already_notified` — mirrors `tasks::agenda_section`'s
/// exact pool-iteration shape and due/overdue logic, but returns structured
/// hits instead of a markdown section, filtered by dedup key. Pure,
/// unit-testable without I/O. Sorted most-overdue first, same as
/// `agenda_section`.
pub fn scan_due_reminders(
    pool: &[(Notebook, Note)],
    today: NaiveDate,
    already_notified: &HashSet<String>,
) -> Vec<DueReminder> {
    let mut out = Vec::new();
    for (notebook, note) in pool {
        for task in tasks::extract(&note.body) {
            if task.done {
                continue;
            }
            let Some(due) = task.due else { continue };
            if due > today {
                continue;
            }
            let key = reminder_key(note, task.occurrence, &task.raw_line);
            if already_notified.contains(&key) {
                continue;
            }
            out.push(DueReminder {
                notebook: notebook.name.clone(),
                note_title: note.frontmatter.title.clone(),
                task_text: task.text.clone(),
                due,
                overdue: due < today,
                key,
            });
        }
    }
    out.sort_by_key(|r| r.due);
    out
}

/// Persisted dedup record for the reminder checker — resets whenever the
/// stored day no longer matches "today", so it never grows unboundedly and
/// each task notifies at most once per calendar day it's due/overdue.
/// Same load/save shape as `LastCapture`, except `load` always returns a
/// usable value (a fresh, empty-for-today state) rather than an `Option` —
/// there's always a sensible default here, unlike a capture-undo record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReminderState {
    pub day: NaiveDate,
    pub notified: Vec<String>,
}

impl ReminderState {
    pub fn load(path: &Path, today: NaiveDate) -> Self {
        Self::load_with_fs(path, today, &crate::fs::LocalFs)
    }

    /// `load`, through an injected `fs` backend instead of always
    /// `LocalFs` — the seam a future non-native caller uses instead.
    pub fn load_with_fs(path: &Path, today: NaiveDate, fs: &dyn crate::fs::FileStore) -> Self {
        let parsed: Option<Self> = fs
            .read_to_string(path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok());
        match parsed {
            Some(state) if state.day == today => state,
            _ => Self {
                day: today,
                notified: Vec::new(),
            },
        }
    }

    pub fn already_notified(&self) -> HashSet<String> {
        self.notified.iter().cloned().collect()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        self.save_with_fs(path, &crate::fs::LocalFs)
    }

    /// `save`, through an injected `fs` backend — see `load_with_fs`.
    pub fn save_with_fs(&self, path: &Path, fs: &dyn crate::fs::FileStore) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs.create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(self)?;
        fs.write(path, contents.as_bytes())?;
        Ok(())
    }
}

/// Fires one real OS desktop notification. Best-effort by every caller (a
/// failure never takes the checker thread down) — see `doctor.rs`'s
/// `notifications_likely_available` for the "will this probably work at
/// all" heuristic instead of trying to surface every backend error to the
/// user. A typed API call, deliberately not a shelled-out
/// `notify-send`/`osascript` string — the notification body is arbitrary
/// user-typed task text, so interpolating it into a shell/AppleScript
/// command line would be real injection surface; passing it as data to a
/// real API call isn't.
#[cfg(feature = "desktop-notify")]
pub fn send_notification(summary: &str, body: &str) -> Result<()> {
    notify_rust::Notification::new()
        .summary(summary)
        .body(body)
        .show()
        .map(|_| ())
        .map_err(|e| crate::Error::Notify(e.to_string()))
}

#[cfg(not(feature = "desktop-notify"))]
pub fn send_notification(_summary: &str, _body: &str) -> Result<()> {
    Err(crate::Error::Notify(
        "desktop notifications not compiled into this build".into(),
    ))
}

/// Cheap heuristic for whether `send_notification` is likely to actually
/// deliver, without sending a real test notification — used by `shiki
/// doctor`, never to gate an actual send attempt. macOS/Windows have no
/// equivalent runtime prerequisite, so they always report available;
/// Linux/BSD need a live D-Bus session, which `notify-rust`'s `zbus`
/// backend silently can't deliver without (exactly the situation a
/// headless server, or this sandbox, is in).
pub fn notifications_likely_available() -> bool {
    #[cfg(target_os = "linux")]
    {
        std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some()
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

/// Held by whichever process spawned the checker (`App` or `shiki
/// daemon`). Both fields are live-updatable without restarting the
/// thread — same "just flip an atomic, never tear the thread down"
/// convention as `shiki-tui`'s `CaptureDaemonHandle`.
pub struct ReminderCheckerHandle {
    pub enabled: Arc<AtomicBool>,
    pub check_interval_secs: Arc<AtomicU64>,
}

/// Spawns the reminder-checker thread. Unlike a capture daemon there's no
/// socket to bind, so this can't fail at spawn time and returns the handle
/// directly rather than a `Result`. `state_path` is passed in rather than
/// resolved here since `shiki-core` deliberately doesn't depend on
/// `shiki-config` — callers get it from
/// `shiki_config::Config::default_reminders_state_path()`.
pub fn spawn_reminder_checker(
    store: NotebookStore,
    state_path: PathBuf,
    initial_interval_secs: u64,
) -> ReminderCheckerHandle {
    let enabled = Arc::new(AtomicBool::new(true));
    let check_interval_secs = Arc::new(AtomicU64::new(initial_interval_secs.max(1)));
    let thread_enabled = Arc::clone(&enabled);
    let thread_interval = Arc::clone(&check_interval_secs);
    std::thread::spawn(move || checker_loop(store, state_path, thread_enabled, thread_interval));
    ReminderCheckerHandle {
        enabled,
        check_interval_secs,
    }
}

/// Sleeps in small 1s steps (rather than one long sleep for the whole
/// interval) specifically so a live interval change via
/// `ReminderCheckerHandle::check_interval_secs` takes effect on the next
/// tick instead of only after whatever the *old* interval used to be.
fn checker_loop(
    store: NotebookStore,
    state_path: PathBuf,
    enabled: Arc<AtomicBool>,
    check_interval_secs: Arc<AtomicU64>,
) {
    let mut elapsed = u64::MAX; // forces an immediate first check
    loop {
        std::thread::sleep(Duration::from_secs(1));
        elapsed = elapsed.saturating_add(1);
        let interval = check_interval_secs.load(Ordering::Relaxed).max(1);
        if elapsed < interval {
            continue;
        }
        elapsed = 0;
        if !enabled.load(Ordering::Relaxed) {
            continue;
        }
        run_one_check(&store, &state_path);
    }
}

fn run_one_check(store: &NotebookStore, state_path: &Path) {
    let today = chrono::Local::now().date_naive();
    let Ok(pool) = store.all_notes() else {
        return;
    };
    let mut state = ReminderState::load(state_path, today);
    let already = state.already_notified();
    let due = scan_due_reminders(&pool, today, &already);
    if due.is_empty() {
        return;
    }
    for reminder in &due {
        let title = if reminder.overdue {
            "Task overdue"
        } else {
            "Task due today"
        };
        let body = format!(
            "{} — {}/{}",
            reminder.task_text, reminder.notebook, reminder.note_title
        );
        let _ = send_notification(title, &body);
        state.notified.push(reminder.key.clone());
    }
    let _ = state.save(state_path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note::Frontmatter;

    fn entry(title: &str, body: &str) -> (Notebook, Note) {
        (
            Notebook::new("nb", PathBuf::from("/tmp/nb")),
            Note::new(
                PathBuf::from(format!("/tmp/nb/{title}.md")),
                Frontmatter::new(title, "nb"),
                body.to_string(),
            ),
        )
    }

    #[test]
    fn scan_due_reminders_finds_due_and_overdue_pending_tasks() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 4).unwrap();
        let pool = vec![
            entry("Bills", "- [ ] pay rent @due(2026-08-01)"),
            entry("Plan", "- [ ] standup @due(2026-08-04)"),
        ];
        let hits = scan_due_reminders(&pool, today, &HashSet::new());
        assert_eq!(hits.len(), 2);
        assert!(hits[0].overdue);
        assert!(!hits[1].overdue);
    }

    #[test]
    fn scan_due_reminders_skips_done_tasks() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 4).unwrap();
        let pool = vec![entry("Bills", "- [x] pay rent @due(2026-08-01)")];
        assert!(scan_due_reminders(&pool, today, &HashSet::new()).is_empty());
    }

    #[test]
    fn scan_due_reminders_skips_future_due_dates() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 4).unwrap();
        let pool = vec![entry("Plan", "- [ ] later @due(2026-12-01)")];
        assert!(scan_due_reminders(&pool, today, &HashSet::new()).is_empty());
    }

    #[test]
    fn scan_due_reminders_skips_already_notified_keys() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 4).unwrap();
        let pool = vec![entry("Bills", "- [ ] pay rent @due(2026-08-01)")];
        let hits = scan_due_reminders(&pool, today, &HashSet::new());
        let mut already = HashSet::new();
        already.insert(hits[0].key.clone());
        assert!(scan_due_reminders(&pool, today, &already).is_empty());
    }

    #[test]
    fn scan_due_reminders_is_empty_for_empty_pool() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 4).unwrap();
        assert!(scan_due_reminders(&[], today, &HashSet::new()).is_empty());
    }

    fn temp_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "shiki-reminders-test-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn reminder_state_save_then_load_round_trips() {
        let dir = temp_path("roundtrip");
        let path = dir.join("reminders-state.toml");
        let today = NaiveDate::from_ymd_opt(2026, 8, 4).unwrap();
        let state = ReminderState {
            day: today,
            notified: vec!["/tmp/nb/Bills.md|0|- [ ] pay rent".to_string()],
        };
        state.save(&path).expect("save must succeed");
        assert_eq!(ReminderState::load(&path, today), state);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reminder_state_load_resets_when_stored_day_is_stale() {
        let dir = temp_path("stale-day");
        let path = dir.join("reminders-state.toml");
        let yesterday = NaiveDate::from_ymd_opt(2026, 8, 3).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 8, 4).unwrap();
        ReminderState {
            day: yesterday,
            notified: vec!["stale-key".to_string()],
        }
        .save(&path)
        .unwrap();
        let loaded = ReminderState::load(&path, today);
        assert_eq!(loaded.day, today);
        assert!(loaded.notified.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reminder_state_load_returns_default_for_missing_file() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 4).unwrap();
        let loaded = ReminderState::load(Path::new("/nonexistent/reminders-state.toml"), today);
        assert_eq!(
            loaded,
            ReminderState {
                day: today,
                notified: Vec::new(),
            }
        );
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn notifications_likely_available_is_true_off_linux() {
        assert!(notifications_likely_available());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn notifications_likely_available_reflects_dbus_session_env_var() {
        // Serialized via a lock so this doesn't race other env-mutating
        // tests in the same binary.
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().unwrap();
        let previous = std::env::var_os("DBUS_SESSION_BUS_ADDRESS");
        unsafe {
            std::env::remove_var("DBUS_SESSION_BUS_ADDRESS");
        }
        assert!(!notifications_likely_available());
        unsafe {
            std::env::set_var("DBUS_SESSION_BUS_ADDRESS", "unix:path=/tmp/fake-bus");
        }
        assert!(notifications_likely_available());
        match previous {
            Some(v) => unsafe { std::env::set_var("DBUS_SESSION_BUS_ADDRESS", v) },
            None => unsafe { std::env::remove_var("DBUS_SESSION_BUS_ADDRESS") },
        }
    }
}
