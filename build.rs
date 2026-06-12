fn main() {
    // Embed an application manifest (asInvoker) into the setup binary.
    // Without it, Windows' installer-detection heuristics kick in on an exe
    // named setup.exe: UAC auto-elevates the whole program (breaking stdout
    // and our own targeted elevation step) and the Program Compatibility
    // Assistant shows "this program might not have installed correctly".
    // Applies to binaries only; the cdylib is unaffected.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_manifest::embed_manifest(embed_manifest::new_manifest("KokoroSapi.Setup"))
            .expect("embedding manifest");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
