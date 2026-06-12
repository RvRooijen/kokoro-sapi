//! Setup tool for kokoro-sapi: downloads assets, registers the voice, sets the
//! default voice and runs a smoke test. Replaces the former PowerShell scripts
//! (no ExecutionPolicy / encoding / manual-elevation friction).
//!
//!   setup install [--user-only] [--quantized]
//!   setup uninstall [--purge]
//!   setup default-voice [TOKEN]
//!   setup test [TOKEN]
//!
//! `install` elevates itself (one UAC prompt) for the machine-wide step that
//! Chrome needs; `--user-only` skips it. The internal `machine-register` /
//! `machine-unregister` subcommands are what the elevated child runs.

mod assets;
mod elevate;
mod registry;
mod speak;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Context, Result};

const DEFAULT_TOKEN: &str = "KokoroHeart";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("");
    let flag = |name: &str| args.iter().any(|a| a == name);
    let token_arg = || {
        args.get(1)
            .filter(|a| !a.starts_with("--"))
            .cloned()
            .unwrap_or_else(|| DEFAULT_TOKEN.to_string())
    };

    let result = match cmd {
        "install" => install(flag("--user-only"), flag("--quantized")),
        "uninstall" => uninstall(flag("--purge")),
        "default-voice" => registry::set_default(&token_arg()),
        "test" => speak::speak_test(&token_arg()),
        // Internal: run elevated, argument 2 is the assets dir.
        "machine-register" => registry::register_machine(&PathBuf::from(
            args.get(1).map(String::as_str).unwrap_or_default(),
        )),
        "machine-unregister" => registry::unregister_machine(),
        _ => {
            eprintln!(
                "usage: setup <install [--user-only] [--quantized] | uninstall [--purge] | default-voice [TOKEN] | test [TOKEN]>"
            );
            return ExitCode::from(2);
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn install(user_only: bool, quantized: bool) -> Result<()> {
    let assets = kokoro_sapi::config::assets_dir(None);

    println!("[1/5] Downloading assets to {} ...", assets.display());
    assets::download_all(&assets, quantized)?;

    println!("[2/5] Installing engine DLL ...");
    let dll = install_dll(&assets)?;

    println!("[3/5] Registering voice (per-user) ...");
    registry::register_user(&dll, &assets)?;

    if user_only {
        println!("[4/5] Skipping machine-wide registration (--user-only); Chrome will not see the voices.");
    } else if elevate::is_elevated() {
        println!("[4/5] Registering machine-wide for Chrome ...");
        registry::register_machine(&assets)?;
    } else {
        println!("[4/5] Registering machine-wide for Chrome (accept the UAC prompt) ...");
        let arg = format!("machine-register \"{}\"", assets.display());
        match elevate::run_self_elevated(&arg) {
            Ok(0) => {}
            Ok(code) => bail!("elevated registration failed with exit code {code}"),
            Err(e) => {
                println!("  skipped ({e}); Chrome will not see the voices. Re-run install to retry.");
            }
        }
    }
    registry::set_default(DEFAULT_TOKEN)?;

    println!("[5/5] Smoke test, you should hear the voice now ...");
    speak::speak_test(DEFAULT_TOKEN)?;

    println!();
    println!("Done. For Chrome: restart it fully (check Task Manager), open reading mode");
    println!("and select \"System text-to-speech voice\".");
    Ok(())
}

/// Copy the engine DLL from next to setup.exe into the assets dir, so the
/// registration survives `cargo clean` or deleting an extracted release zip.
fn install_dll(assets: &std::path::Path) -> Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let source = exe
        .parent()
        .map(|d| d.join("kokoro_sapi.dll"))
        .filter(|p| p.exists())
        .context("kokoro_sapi.dll not found next to setup.exe - build with `cargo build --release` first")?;
    let dest = assets.join("kokoro_sapi.dll");
    if let Err(e) = std::fs::copy(&source, &dest) {
        bail!(
            "could not copy DLL to {} ({e}); close apps that are using the voice (Chrome!) and retry",
            dest.display()
        );
    }
    Ok(dest)
}

fn uninstall(purge: bool) -> Result<()> {
    registry::unregister_user()?;
    if registry::machine_tokens_exist() {
        println!("Removing machine-wide registration (accept the UAC prompt) ...");
        match elevate::run_self_elevated("machine-unregister") {
            Ok(0) => {}
            Ok(code) => println!("  elevated cleanup exited with code {code}"),
            Err(e) => println!("  skipped ({e}); machine-wide tokens are still registered"),
        }
    }
    let assets = kokoro_sapi::config::assets_dir(None);
    if purge {
        if assets.exists() {
            std::fs::remove_dir_all(&assets)
                .with_context(|| format!("removing {}", assets.display()))?;
            println!("Removed {}", assets.display());
        }
    } else {
        println!("Assets left in {} (use --purge to delete)", assets.display());
    }
    println!("Unregistered.");
    Ok(())
}
