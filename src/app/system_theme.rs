//! Detect whether the desktop prefers a dark colour scheme (for
//! View ▸ Theme ▸ "Use system setting").
//!
//! winit doesn't report the theme on X11, and on Wayland only reports
//! overrides, so we ask the XDG desktop portal directly
//! (`org.freedesktop.appearance color-scheme`, implemented by KDE and GNOME)
//! through whichever D-Bus command-line tool the host has. KDE's and
//! GNOME's own settings are fallbacks for desktops without the portal. A
//! background thread re-checks every few seconds so switching the desktop
//! between light and dark is followed without a restart.

use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::{Duration, Instant};

const UNKNOWN: u8 = 0;
const LIGHT: u8 = 1;
const DARK: u8 = 2;

static STATE: AtomicU8 = AtomicU8::new(UNKNOWN);
static WANTED: AtomicBool = AtomicBool::new(false);
static STARTED: AtomicBool = AtomicBool::new(false);

/// What the desktop prefers, if known.
pub fn prefers_dark() -> Option<bool> {
    match STATE.load(Ordering::Relaxed) {
        DARK => Some(true),
        LIGHT => Some(false),
        _ => None,
    }
}

/// Tell the watcher whether anyone is using the system setting (so it
/// doesn't spawn processes for nothing).
pub fn set_wanted(w: bool) {
    WANTED.store(w, Ordering::Relaxed);
}

/// Start polling in the background (real app only; tests never call this).
pub fn start_watcher(ctx: egui::Context) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = std::thread::Builder::new().name("system-theme".into()).spawn(move || {
        let mut first = true;
        loop {
            if first || WANTED.load(Ordering::Relaxed) {
                let v = match detect() {
                    Some(true) => DARK,
                    Some(false) => LIGHT,
                    None => UNKNOWN,
                };
                if STATE.swap(v, Ordering::Relaxed) != v {
                    ctx.request_repaint();
                }
                first = false;
            }
            std::thread::sleep(Duration::from_secs(4));
        }
    });
}

/// Ask the desktop now. `Some(true)` = dark.
pub fn detect() -> Option<bool> {
    if let Some(v) = std::env::var("PROJECTFCG_SYSTEM_THEME").ok().and_then(|v| parse_word(&v)) {
        return Some(v);
    }
    // Windows and macOS report their theme through the window (winit), which
    // the app reads when this says nothing; only Linux desktops need asking.
    if !cfg!(target_os = "linux") {
        return None;
    }
    portal().or_else(gtk_env).or_else(kde_globals).or_else(gsettings)
}

fn parse_word(v: &str) -> Option<bool> {
    match v.trim().to_ascii_lowercase().as_str() {
        "dark" => Some(true),
        "light" => Some(false),
        _ => None,
    }
}

const PORTAL_DEST: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";

fn portal() -> Option<bool> {
    let tries: [&[&str]; 4] = [
        &["gdbus", "call", "--session", "--timeout", "2", "--dest", PORTAL_DEST, "--object-path", PORTAL_PATH,
          "--method", "org.freedesktop.portal.Settings.ReadOne", "org.freedesktop.appearance", "color-scheme"],
        // portals older than v2 only have Read
        &["gdbus", "call", "--session", "--timeout", "2", "--dest", PORTAL_DEST, "--object-path", PORTAL_PATH,
          "--method", "org.freedesktop.portal.Settings.Read", "org.freedesktop.appearance", "color-scheme"],
        &["busctl", "--user", "--timeout=2", "call", PORTAL_DEST, PORTAL_PATH, "org.freedesktop.portal.Settings",
          "Read", "ss", "org.freedesktop.appearance", "color-scheme"],
        &["dbus-send", "--session", "--print-reply=literal", "--reply-timeout=2000", "--dest=org.freedesktop.portal.Desktop",
          PORTAL_PATH, "org.freedesktop.portal.Settings.Read", "string:org.freedesktop.appearance", "string:color-scheme"],
    ];
    for args in tries {
        if let Some(out) = run(args) {
            if let Some(v) = parse_portal_reply(&out) {
                return v;
            }
        }
    }
    None
}

/// Parse the reply of any of the tools above. The value is the last
/// unsigned integer: 1 = prefer dark, 2 = prefer light, 0 = no preference.
/// Returns `None` if the reply has no value (try the next tool) and
/// `Some(None)` for "no preference" (stop, but fall back to desktop files).
pub fn parse_portal_reply(out: &str) -> Option<Option<bool>> {
    let n = out
        .split(|c: char| !c.is_ascii_digit())
        .filter(|t| !t.is_empty())
        .last()?
        .parse::<u32>()
        .ok()?;
    // "uint32"/"u" type markers contain digits too ("uint32" → 32): require
    // the number to be the last token and a valid value
    match n {
        1 => Some(Some(true)),
        2 => Some(Some(false)),
        0 => Some(None),
        _ => None,
    }
}

fn gtk_env() -> Option<bool> {
    let t = std::env::var("GTK_THEME").ok()?;
    t.to_ascii_lowercase().contains(":dark").then_some(true)
}

fn kde_globals() -> Option<bool> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))?;
    let text = std::fs::read_to_string(base.join("kdeglobals")).ok()?;
    parse_kdeglobals(&text)
}

/// KDE: the window background colour decides; the scheme name is a hint.
pub fn parse_kdeglobals(text: &str) -> Option<bool> {
    let mut section = "";
    let mut scheme_dark = None;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            section = line;
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        if section == "[Colors:Window]" && k.trim() == "BackgroundNormal" {
            let c: Vec<u8> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            if c.len() >= 3 {
                let lum = super::theme::rel_luminance(egui::Color32::from_rgb(c[0], c[1], c[2]));
                return Some(lum < 0.2);
            }
        }
        if section == "[General]" && k.trim() == "ColorScheme" {
            scheme_dark = Some(v.to_ascii_lowercase().contains("dark"));
        }
    }
    scheme_dark
}

fn gsettings() -> Option<bool> {
    let out = run(&["gsettings", "get", "org.gnome.desktop.interface", "color-scheme"])?;
    if out.contains("prefer-dark") {
        Some(true)
    } else if out.contains("default") || out.contains("prefer-light") {
        Some(false)
    } else {
        None
    }
}

/// Run a command for at most 3 s; stdout on success.
fn run(args: &[&str]) -> Option<String> {
    let mut child = Command::new(args[0])
        .args(&args[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let t0 = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(st)) => {
                if !st.success() {
                    return None;
                }
                let mut s = String::new();
                use std::io::Read;
                child.stdout.take()?.read_to_string(&mut s).ok()?;
                return Some(s);
            }
            Ok(None) if t0.elapsed() < Duration::from_secs(3) => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portal_replies_from_each_tool() {
        assert_eq!(parse_portal_reply("(<uint32 1>,)\n"), Some(Some(true)), "gdbus ReadOne");
        assert_eq!(parse_portal_reply("(<<uint32 2>>,)\n"), Some(Some(false)), "gdbus Read");
        assert_eq!(parse_portal_reply("v v u 1\n"), Some(Some(true)), "busctl Read");
        assert_eq!(parse_portal_reply("v u 0\n"), Some(None), "no preference");
        assert_eq!(parse_portal_reply("   variant       variant          uint32 1\n"), Some(Some(true)), "dbus-send");
        assert_eq!(parse_portal_reply(""), None);
        assert_eq!(parse_portal_reply("Error: no such interface"), None);
    }
    #[test]
    fn kde_globals() {
        let dark = "[General]\nColorScheme=BreezeDark\n\n[Colors:Window]\nBackgroundNormal=32,35,38\n";
        assert_eq!(parse_kdeglobals(dark), Some(true));
        let light = "[Colors:Window]\nBackgroundNormal=239,240,241\n[General]\nColorScheme=BreezeLight\n";
        assert_eq!(parse_kdeglobals(light), Some(false));
        assert_eq!(parse_kdeglobals("[General]\nColorScheme=VapourDark\n"), Some(true));
        assert_eq!(parse_kdeglobals("[KDE]\nSingleClick=false\n"), None);
    }
    #[test]
    fn env_override() {
        assert_eq!(parse_word("Dark"), Some(true));
        assert_eq!(parse_word(" light "), Some(false));
        assert_eq!(parse_word("blue"), None);
    }
}
