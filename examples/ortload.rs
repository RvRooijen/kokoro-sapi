//! Standalone ORT smoke test: load the Kokoro ONNX session in a plain console
//! process (no COM/SAPI/STA). Originally the repro for the ETW telemetry
//! deadlock in ORT < 1.26 (see download-assets.ps1); keep it around to sanity
//! check onnxruntime.dll when bumping ORT or ort-crate versions:
//!
//!   cargo run --release --example ortload
//!   ORTTEST_DLL=C:\path\other\onnxruntime.dll cargo run --release --example ortload

use std::path::PathBuf;
use std::time::Instant;

use ort::session::Session;

fn main() -> anyhow::Result<()> {
    let assets = PathBuf::from(std::env::var("LOCALAPPDATA")?).join("KokoroSapi");
    // Allow overriding just the onnxruntime.dll for testing different ORT versions.
    let dll = match std::env::var("ORTTEST_DLL") {
        Ok(p) => PathBuf::from(p),
        Err(_) => assets.join("onnxruntime.dll"),
    };
    std::env::set_var("ORT_DYLIB_PATH", &dll);
    eprintln!("ORT_DYLIB_PATH = {}", dll.display());
    eprintln!("dll exists      = {}", dll.exists());

    let model = assets.join("model.onnx");
    eprintln!("model           = {} (exists={})", model.display(), model.exists());

    eprintln!("[{:?}] ort::init().with_telemetry(false).commit()...", Instant::now());
    let committed = ort::init().with_telemetry(false).commit();
    eprintln!("[{:?}] env committed = {committed}", Instant::now());

    eprintln!("[{:?}] Session::builder()...", Instant::now());
    let mut builder = Session::builder()?;
    eprintln!("[{:?}] commit_from_file()...", Instant::now());
    let session = builder.commit_from_file(&model)?;
    eprintln!("[{:?}] loaded.", Instant::now());

    let names: Vec<String> = session.inputs().iter().map(|i| i.name().to_string()).collect();
    eprintln!("inputs = {names:?}");
    eprintln!("OK");
    Ok(())
}
