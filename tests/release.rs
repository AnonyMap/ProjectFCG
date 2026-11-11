//! Release hygiene. (Whether Cargo.lock matches the version can't be
//! tested here: `cargo test` rewrites a stale lock before any test runs.
//! The pull-request check, .github/workflows/check.yml, runs with
//! `--locked` as the release build does.)

fn read(path: &str) -> String {
    std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path)).unwrap()
}

/// The version in Cargo.toml (read now, not when this test was built).
fn version() -> String {
    let toml = read("Cargo.toml");
    let line = toml.lines().find(|l| l.starts_with("version = ")).unwrap();
    line.split('"').nth(1).unwrap().to_string()
}

#[test]
fn the_changelog_has_this_version() {
    let v = version();
    let short: String = v.split('.').take(2).collect::<Vec<_>>().join(".");
    let log = read("docs/CHANGELOG.md");
    let first = log.lines().find(|l| l.starts_with("## ")).unwrap_or("");
    assert!(first.starts_with(&format!("## {short}")) || first.starts_with(&format!("## {v}")), "the newest changelog entry is {first:?}, not {v}");
}
