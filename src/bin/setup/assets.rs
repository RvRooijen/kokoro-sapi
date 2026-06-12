//! Downloads the runtime assets: Kokoro model + tokenizer + voice styles from
//! Hugging Face, onnxruntime.dll from the Microsoft release zip, and
//! espeak-ng.dll + data from the piper release zip (which conveniently bundles
//! a ready-to-use Windows build).

use std::fs::File;
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

const HF: &str = "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main";
// 1.26+ required: older ORT deadlocks in CreateEnv on ETW telemetry registration.
const ORT_VERSION: &str = "1.26.0";
const PIPER_ZIP: &str =
    "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_windows_amd64.zip";

pub fn download_all(dir: &Path, quantized: bool) -> Result<()> {
    std::fs::create_dir_all(dir.join("voices"))?;

    let model = if quantized { "model_quantized.onnx" } else { "model.onnx" };
    fetch(&format!("{HF}/onnx/{model}"), &dir.join("model.onnx"))?;
    fetch(&format!("{HF}/tokenizer.json"), &dir.join("tokenizer.json"))?;
    for v in &super::registry::VOICES {
        fetch(
            &format!("{HF}/voices/{}.bin", v.voice_name),
            &dir.join("voices").join(format!("{}.bin", v.voice_name)),
        )?;
    }

    if !dir.join("onnxruntime.dll").exists() {
        let url = format!(
            "https://github.com/microsoft/onnxruntime/releases/download/v{ORT_VERSION}/onnxruntime-win-x64-{ORT_VERSION}.zip"
        );
        let zip_path = dir.join("_ort.zip");
        fetch(&url, &zip_path)?;
        let prefix = format!("onnxruntime-win-x64-{ORT_VERSION}/lib/");
        extract(&zip_path, &prefix, dir, &["onnxruntime.dll", "onnxruntime_providers_shared.dll"])?;
        std::fs::remove_file(zip_path)?;
    }

    if !dir.join("espeak-ng.dll").exists() {
        let zip_path = dir.join("_piper.zip");
        fetch(PIPER_ZIP, &zip_path)?;
        extract_tree(&zip_path, "piper/", dir, &["espeak-ng.dll", "espeak-ng-data/"])?;
        std::fs::remove_file(zip_path)?;
    }

    Ok(())
}

/// Download via curl.exe, which ships with Windows 10/11: schannel TLS, a
/// progress bar and resume (`-C -` continues a partial .part file) for free.
fn fetch(url: &str, dest: &Path) -> Result<()> {
    let name = dest.file_name().unwrap_or_default().to_string_lossy().to_string();
    if dest.exists() {
        println!("  exists: {name}");
        return Ok(());
    }
    println!("  {name}:");
    let tmp = dest.with_extension("part");
    let status = Command::new("curl.exe")
        .args(["--location", "--fail", "--progress-bar", "--continue-at", "-"])
        .arg("--output")
        .arg(&tmp)
        .arg(url)
        .status()
        .context("running curl.exe (ships with Windows 10/11)")?;
    if !status.success() {
        bail!("download of {name} failed ({status}); re-run to resume");
    }
    std::fs::rename(&tmp, dest)?;
    Ok(())
}

/// Extract specific files from `prefix` in the zip into `dir` (flat).
fn extract(zip_path: &Path, prefix: &str, dir: &Path, names: &[&str]) -> Result<()> {
    let mut archive = zip::ZipArchive::new(File::open(zip_path)?)?;
    for name in names {
        let entry = format!("{prefix}{name}");
        let mut file = match archive.by_name(&entry) {
            Ok(f) => f,
            Err(_) => continue, // optional entries (providers_shared may be absent)
        };
        let mut out = File::create(dir.join(name))?;
        std::io::copy(&mut file, &mut out)?;
        println!("  extracted: {name}");
    }
    Ok(())
}

/// Extract files/subtrees from `prefix` in the zip into `dir`, keeping their
/// relative paths. A pattern ending in '/' matches a whole directory tree.
fn extract_tree(zip_path: &Path, prefix: &str, dir: &Path, patterns: &[&str]) -> Result<()> {
    let mut archive = zip::ZipArchive::new(File::open(zip_path)?)?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if file.is_dir() {
            continue;
        }
        let name = file.name().to_string();
        let Some(rel) = name.strip_prefix(prefix) else { continue };
        let matched = patterns
            .iter()
            .any(|p| if p.ends_with('/') { rel.starts_with(p) } else { rel == *p });
        if !matched || rel.contains("..") {
            continue;
        }
        let dest = dir.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = File::create(&dest)?;
        std::io::copy(&mut file, &mut out)?;
    }
    println!("  extracted: espeak-ng.dll + espeak-ng-data");
    Ok(())
}
