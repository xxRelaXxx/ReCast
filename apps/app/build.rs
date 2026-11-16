fn main() {
    println!("cargo:rerun-if-changed=assets/recast.ico");
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/recast.ico");
        res.set("ProductName", "ReCast");
        res.set("FileDescription", "ReCast desktop app");
        res.set("CompanyName", "ReCast contributors");
        res.set("LegalCopyright", "MIT License");
        res.set("OriginalFilename", "recast-app.exe");
        res.set("ProductVersion", env!("CARGO_PKG_VERSION"));
        res.set("FileVersion", env!("CARGO_PKG_VERSION"));
        res.compile().expect("failed to compile Windows resources");
    }
}
