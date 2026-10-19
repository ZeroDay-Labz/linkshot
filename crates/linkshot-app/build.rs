fn main() {
    // Windows: application icon and version information (packaging/windows/linkshot.rc).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=../../packaging/windows/linkshot.rc");
        println!("cargo:rerun-if-changed=../../packaging/icons/linkshot.ico");
        let result = embed_resource::compile("../../packaging/windows/linkshot.rc", embed_resource::NONE);
        if !matches!(result, embed_resource::CompilationResult::Ok) {
            println!("cargo:warning=Windows resources (icon, version info) not embedded: {result:?}");
        }
    }
}
