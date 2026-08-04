// Embed Windows application icon into the PE (taskbar / Explorer / shortcuts).

fn main() {
    if std::env::var("CARGO_CFG_WINDOWS").is_err() {
        return;
    }
    // Only when building with the gui feature (tray / window app).
    if std::env::var("CARGO_FEATURE_GUI").is_err() {
        return;
    }

    let icon = std::path::Path::new("assets/tluw-icon.ico");
    if !icon.exists() {
        println!("cargo:warning=missing {icon:?}; PE icon not embedded");
        return;
    }

    let mut res = winres::WindowsResource::new();
    res.set_icon(icon.to_str().unwrap());
    res.set("ProductName", "Telemetry and Logging Utility for Windows");
    res.set("FileDescription", "Telemetry and Logging Utility for Windows");
    res.set("CompanyName", "Chillcoders LLC");
    res.set("LegalCopyright", "Copyright (c) 2026 James Guan / Chillcoders LLC");
    if let Err(e) = res.compile() {
        println!("cargo:warning=winres failed: {e}");
    }
}
