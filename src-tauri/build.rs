fn main() {
    assert!(
        std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux")
            || std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("x86_64"),
        "Linux 内置组件仅支持 x86_64"
    );
    tauri_build::build()
}
