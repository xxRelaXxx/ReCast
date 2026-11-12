fn main() {
    println!("cargo:rerun-if-changed=assets/rewebcam.ico");
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/rewebcam.ico");
        res.set("ProductName", "Rewebcam");
        res.set("FileDescription", "Rewebcam launcher");
        res.set("CompanyName", "Rewebcam contributors");
        res.set("LegalCopyright", "MIT License");
        res.set("OriginalFilename", "Rewebcam.exe");
        res.set("ProductVersion", env!("CARGO_PKG_VERSION"));
        res.set("FileVersion", env!("CARGO_PKG_VERSION"));
        res.compile().expect("failed to compile Windows resources");
    }
}
