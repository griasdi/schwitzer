//! Embeds the app icon and file properties into the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=assets/logo/schwitzer.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/logo/schwitzer.ico");
        resource.set("ProductName", "Schwitzer");
        // Task Manager shows this as the process name.
        resource.set("FileDescription", "Schwitzer");
        resource
            .compile()
            .expect("failed to embed the Windows resources");
    }
}
