//! Crash safety that doesn't need a window: atomic file writes,
//! AutoRecover copies of unsaved work, and crash reports.
//!
//! Each running window is a *session* with its own files in
//! `<state dir>/recovery/`:
//!
//! * `<session>.pfcg`: the plan as it was at the last AutoRecover save
//! * `<session>.meta.json`: where it came from and when it was saved
//! * `<session>.alive`: touched every 30 s by a heartbeat thread
//!
//! A clean exit removes all three. Files whose heartbeat stopped more
//! than two minutes ago belong to a window that crashed or was killed;
//! the next start offers them back. A second window that's still running
//! keeps its heartbeat fresh, so its work is never offered as "lost".

use crate::model::Project;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// A heartbeat older than this means the window is gone.
pub const STALE_AFTER: Duration = Duration::from_secs(120);
pub const HEARTBEAT_EVERY: Duration = Duration::from_secs(30);

/// Per-user state (crash reports, recovery copies):
/// `$XDG_STATE_HOME/projectfcg` (default `~/.local/state/projectfcg`) on
/// Linux, `%LOCALAPPDATA%\ProjectFCG` on Windows and
/// `~/Library/Application Support/ProjectFCG` on macOS.
pub fn state_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        return std::env::var_os("LOCALAPPDATA").or_else(|| std::env::var_os("APPDATA")).map(|a| PathBuf::from(a).join("ProjectFCG"));
    }
    if cfg!(target_os = "macos") {
        return std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support/ProjectFCG"));
    }
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
    Some(base.join("projectfcg"))
}

/// Write a file so that a crash or power cut leaves either the old file
/// or the new one, never a truncated mix: write a temporary file beside
/// it, flush it to disk, then rename it over the target.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// What a recovery copy is a copy of.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecoveryMeta {
    /// The file the plan was opened from or last saved to (None: never saved).
    pub original: Option<PathBuf>,
    /// Shown in the recovery list: the file's name or the project title.
    pub title: String,
    /// When the copy was written (local time).
    pub saved_at: chrono::NaiveDateTime,
    pub app_version: String,
}

/// A recovery copy left by a window that didn't close normally.
#[derive(Clone, Debug, PartialEq)]
pub struct Orphan {
    pub session: String,
    pub plan: PathBuf,
    pub meta: RecoveryMeta,
}

/// The recovery folder.
#[derive(Clone, Debug)]
pub struct Recovery {
    pub dir: PathBuf,
}

impl Recovery {
    pub fn new(dir: PathBuf) -> Self {
        Recovery { dir }
    }

    /// `<state dir>/recovery`.
    pub fn default_location() -> Option<Self> {
        state_dir().map(|d| Recovery::new(d.join("recovery")))
    }

    fn file(&self, session: &str, ext: &str) -> PathBuf {
        self.dir.join(format!("{session}.{ext}"))
    }

    /// Save a recovery copy for a session (atomically).
    pub fn write(&self, session: &str, json: &str, meta: &RecoveryMeta) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        write_atomic(&self.file(session, "pfcg"), json.as_bytes())?;
        let m = serde_json::to_string_pretty(meta).map_err(std::io::Error::other)?;
        write_atomic(&self.file(session, "meta.json"), m.as_bytes())?;
        self.touch(session)
    }

    /// Record that the session's window is still running.
    pub fn touch(&self, session: &str) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        std::fs::write(self.file(session, "alive"), b"")
    }

    /// Remove a session's files (it closed normally, or its work was saved
    /// or recovered).
    pub fn remove(&self, session: &str) {
        for ext in ["pfcg", "meta.json", "alive"] {
            let _ = std::fs::remove_file(self.file(session, ext));
        }
    }

    /// Recovery copies whose window is gone, newest first. `now` is passed
    /// in so tests can age heartbeats.
    pub fn orphans(&self, now: SystemTime) -> Vec<Orphan> {
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return vec![] };
        let mut v = vec![];
        for e in rd.flatten() {
            let path = e.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
            let Some(session) = name.strip_suffix(".pfcg") else { continue };
            let alive = std::fs::metadata(self.file(session, "alive")).and_then(|m| m.modified()).ok();
            let gone = match alive {
                Some(t) => now.duration_since(t).map(|d| d > STALE_AFTER).unwrap_or(false),
                None => true,
            };
            if !gone {
                continue;
            }
            let meta = std::fs::read_to_string(self.file(session, "meta.json"))
                .ok()
                .and_then(|s| serde_json::from_str::<RecoveryMeta>(&s).ok())
                .unwrap_or_else(|| RecoveryMeta {
                    original: None,
                    title: "Unsaved project".into(),
                    saved_at: chrono::DateTime::<chrono::Local>::from(e.metadata().and_then(|m| m.modified()).unwrap_or(now)).naive_local(),
                    app_version: String::new(),
                });
            v.push(Orphan { session: session.to_string(), plan: path.clone(), meta });
        }
        v.sort_by(|a, b| b.meta.saved_at.cmp(&a.meta.saved_at));
        v
    }

    /// Load a recovery copy.
    pub fn load(&self, o: &Orphan) -> Result<Project, String> {
        crate::io::load(&o.plan)
    }
}

/// A new session id: unique per window (process id and start time).
pub fn new_session_id() -> String {
    let ms = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    format!("{}-{ms}", std::process::id())
}

/// Keep a session's heartbeat fresh from a background thread for as long
/// as the process runs. A crash or kill stops it, which is the point.
pub fn start_heartbeat(rec: Recovery, session: String) {
    let _ = std::thread::Builder::new().name("autorecover-heartbeat".into()).spawn(move || loop {
        let _ = rec.touch(&session);
        std::thread::sleep(HEARTBEAT_EVERY);
    });
}

// ---------------------------------------------------------------------------
// Crash reports

static LAST_PANIC: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Install a panic hook that remembers the message, where it happened and
/// a backtrace, for the crash report, then prints as usual.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let bt = std::backtrace::Backtrace::force_capture();
        let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
        let text = format!("{}\nthread: {thread}\n\nbacktrace:\n{bt}", describe_panic(info));
        if let Ok(mut slot) = LAST_PANIC.lock() {
            *slot = Some(text);
        }
        previous(info);
    }));
}

fn describe_panic(info: &std::panic::PanicHookInfo<'_>) -> String {
    let msg = payload_text(info.payload());
    match info.location() {
        Some(l) => format!("panic: {msg}\nat: {}:{}:{}", l.file(), l.line(), l.column()),
        None => format!("panic: {msg}"),
    }
}

/// The text of a panic payload.
pub fn payload_text(p: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = p.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = p.downcast_ref::<String>() {
        s.clone()
    } else {
        "(no message)".into()
    }
}

/// The last panic the hook saw (taken), or a description of `payload`
/// when no hook is installed (tests).
pub fn take_panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    LAST_PANIC.lock().ok().and_then(|mut s| s.take()).unwrap_or_else(|| format!("panic: {}", payload_text(payload)))
}

/// Write a crash report into `<dir>/crash-reports/`. Returns its path.
/// The report says what went wrong and where, never the plan's contents.
pub fn write_crash_report(dir: &Path, context: &str, panic: &str) -> Option<PathBuf> {
    let reports = dir.join("crash-reports");
    std::fs::create_dir_all(&reports).ok()?;
    let now = chrono::Local::now();
    let path = reports.join(format!("crash-{}-{}.txt", now.format("%Y%m%d-%H%M%S"), std::process::id()));
    let text = format!(
        "ProjectFCG {} crash report\ntime: {}\nos: {} {}\n\n{context}\n\n{panic}\n",
        env!("CARGO_PKG_VERSION"),
        now.format("%Y-%m-%d %H:%M:%S %z"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    );
    write_atomic(&path, text.as_bytes()).ok()?;
    // keep the 20 newest
    if let Ok(rd) = std::fs::read_dir(&reports) {
        let mut all: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "txt")).collect();
        all.sort();
        let n = all.len();
        for old in all.into_iter().take(n.saturating_sub(20)) {
            let _ = std::fs::remove_file(old);
        }
    }
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pfcg-rec-{tag}-{}", new_session_id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn atomic_write_replaces_and_leaves_no_temp() {
        let d = tmpdir("atomic");
        let f = d.join("plan.pfcg");
        write_atomic(&f, b"one").unwrap();
        write_atomic(&f, b"two").unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"two");
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 1, "no temporary file left");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn live_sessions_are_not_orphans() {
        let d = tmpdir("orphans");
        let r = Recovery::new(d.clone());
        let meta = RecoveryMeta { original: None, title: "Plan".into(), saved_at: chrono::Local::now().naive_local(), app_version: "x".into() };
        r.write("a", "{}", &meta).unwrap();
        let now = SystemTime::now();
        assert!(r.orphans(now).is_empty(), "heartbeat is fresh");
        assert_eq!(r.orphans(now + Duration::from_secs(600)).len(), 1, "heartbeat stopped");
        r.remove("a");
        assert!(r.orphans(now + Duration::from_secs(600)).is_empty());
        let _ = std::fs::remove_dir_all(d);
    }
}
