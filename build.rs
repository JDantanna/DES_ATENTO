fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Des_Atento");
        res.set("FileDescription", "Des_Atento");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icone nao embutido: {e}");
        }
    }
}
