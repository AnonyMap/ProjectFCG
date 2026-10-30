//! Build script: gives the Windows .exe its icon and version details.
//! (Nothing to do on other platforms.)

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=packaging/projectfcg.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("packaging/projectfcg.ico");
        res.set("ProductName", "ProjectFCG");
        res.set("FileDescription", "ProjectFCG project scheduler");
        res.set("LegalCopyright", "MIT License");
        // a missing resource compiler shouldn't stop the build: the exe
        // just has the default icon
        if let Err(e) = res.compile() {
            println!("cargo:warning=couldn't embed the Windows icon: {e}");
        }
    }
}
