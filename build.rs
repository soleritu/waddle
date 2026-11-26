// Gives waddle.exe its penguin icon and version details, so Explorer, the taskbar, and shortcuts show them.
fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("src/waddle.ico");
        res.set("ProductName", "Waddle");
        res.set("FileDescription", "Waddle, the local AI sandbox");
        res.set("CompanyName", "Waddle");
        res.set("LegalCopyright", "Copyright 2026 soleritu. PolyForm Noncommercial 1.0.0");
        if let Err(e) = res.compile() {
            println!("cargo:warning=No icon resource: {e}");
        }
    }
}
