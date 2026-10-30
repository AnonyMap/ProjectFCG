//! Write the samples as MS Project XML (for checking with other tools).
fn main() {
    let which = std::env::args().nth(1).unwrap_or_default();
    let p = if which == "agile" { projectfcg::app::sample::agile_release() } else { projectfcg::app::sample::software_release() };
    print!("{}", projectfcg::mspdi::to_xml(&p));
}
