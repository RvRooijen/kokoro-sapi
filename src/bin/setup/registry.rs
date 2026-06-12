//! Registry plumbing: COM class, SAPI/OneCore voice tokens, default voice.
//!
//! Layout (see README "Chrome reading mode" for the why):
//! - COM class: HKCU\Software\Classes\CLSID\{clsid} (per-user, no admin)
//! - Voice tokens: HKCU SAPI always; HKLM SAPI + HKLM OneCore via the elevated
//!   machine step (Chrome only reads the OneCore list)
//! - Defaults: DefaultTokenId under HKCU Speech\Voices (classic SAPI) and HKCU
//!   Speech_OneCore\Voices (Chrome picks the first = default OneCore token)

use std::path::Path;

use anyhow::{bail, Context, Result};
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use winreg::RegKey;

pub struct VoiceDef {
    pub token: &'static str,
    pub display: &'static str,
    pub voice_name: &'static str,
    pub gender: &'static str,
    /// LANGID hex string: 409 = en-US, 809 = en-GB.
    pub language: &'static str,
}

pub const VOICES: [VoiceDef; 4] = [
    VoiceDef { token: "KokoroHeart", display: "Kokoro Heart (en-US)", voice_name: "af_heart", gender: "Female", language: "409" },
    VoiceDef { token: "KokoroBella", display: "Kokoro Bella (en-US)", voice_name: "af_bella", gender: "Female", language: "409" },
    VoiceDef { token: "KokoroMichael", display: "Kokoro Michael (en-US)", voice_name: "am_michael", gender: "Male", language: "409" },
    VoiceDef { token: "KokoroEmma", display: "Kokoro Emma (en-GB)", voice_name: "bf_emma", gender: "Female", language: "809" },
];

const SAPI_TOKENS: &str = r"SOFTWARE\Microsoft\Speech\Voices\Tokens";
const ONECORE_TOKENS: &str = r"SOFTWARE\Microsoft\Speech_OneCore\Voices\Tokens";

fn write_tokens(root: &RegKey, tokens_path: &str, assets: &Path) -> Result<()> {
    let clsid = kokoro_sapi::clsid_braced();
    for v in &VOICES {
        let (key, _) = root
            .create_subkey(format!(r"{tokens_path}\{}", v.token))
            .with_context(|| format!("creating token {}", v.token))?;
        key.set_value("", &v.display)?;
        key.set_value("CLSID", &clsid)?;
        key.set_value("VoiceName", &v.voice_name)?;
        key.set_value("AssetsDir", &assets.to_string_lossy().to_string())?;
        let (attrs, _) = key.create_subkey("Attributes")?;
        attrs.set_value("Name", &v.display)?;
        attrs.set_value("Gender", &v.gender)?;
        attrs.set_value("Age", &"Adult")?;
        attrs.set_value("Vendor", &"Kokoro")?;
        // Chrome skips tokens without a Language attribute (tts_win.cc).
        attrs.set_value("Language", &v.language)?;
        println!("  voice: {}", v.display);
    }
    Ok(())
}

fn remove_tokens(root: &RegKey, tokens_path: &str) {
    for v in &VOICES {
        let _ = root.delete_subkey_all(format!(r"{tokens_path}\{}", v.token));
    }
}

pub fn register_user(dll: &Path, assets: &Path) -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let clsid = kokoro_sapi::clsid_braced();
    let (inproc, _) = hkcu
        .create_subkey(format!(r"Software\Classes\CLSID\{clsid}\InprocServer32"))
        .context("creating COM class key")?;
    inproc.set_value("", &dll.to_string_lossy().to_string())?;
    inproc.set_value("ThreadingModel", &"Both")?;
    write_tokens(&hkcu, SAPI_TOKENS, assets)
}

pub fn register_machine(assets: &Path) -> Result<()> {
    if assets.as_os_str().is_empty() {
        bail!("machine-register needs the assets dir as argument");
    }
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    write_tokens(&hklm, SAPI_TOKENS, assets)?;
    write_tokens(&hklm, ONECORE_TOKENS, assets)
}

pub fn unregister_user() -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let clsid = kokoro_sapi::clsid_braced();
    let _ = hkcu.delete_subkey_all(format!(r"Software\Classes\CLSID\{clsid}"));
    remove_tokens(&hkcu, SAPI_TOKENS);
    // Clear defaults that point at a Kokoro token.
    for voices_key in [r"SOFTWARE\Microsoft\Speech\Voices", r"SOFTWARE\Microsoft\Speech_OneCore\Voices"] {
        if let Ok(key) = hkcu.open_subkey_with_flags(voices_key, winreg::enums::KEY_ALL_ACCESS) {
            let default: String = key.get_value("DefaultTokenId").unwrap_or_default();
            if default.contains(r"\Tokens\Kokoro") {
                let _ = key.delete_value("DefaultTokenId");
            }
        }
    }
    Ok(())
}

pub fn unregister_machine() -> Result<()> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    remove_tokens(&hklm, SAPI_TOKENS);
    remove_tokens(&hklm, ONECORE_TOKENS);
    Ok(())
}

pub fn machine_tokens_exist() -> bool {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    VOICES.iter().any(|v| {
        hklm.open_subkey(format!(r"{ONECORE_TOKENS}\{}", v.token)).is_ok()
            || hklm.open_subkey(format!(r"{SAPI_TOKENS}\{}", v.token)).is_ok()
    })
}

/// Sets the per-user default voice for classic SAPI, and for OneCore when the
/// machine-wide token exists. The OneCore default is what makes Chrome's
/// "System text-to-speech voice" resolve to Kokoro for en-US (SAPI enumerates
/// the default token first; reading mode keeps one system voice per language).
pub fn set_default(token: &str) -> Result<()> {
    if !VOICES.iter().any(|v| v.token == token) {
        bail!(
            "unknown voice token {token:?}; available: {}",
            VOICES.map(|v| v.token).join(", ")
        );
    }
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey(format!(r"{SAPI_TOKENS}\{token}"))
        .context("voice is not registered - run `setup install` first")?;

    let (key, _) = hkcu.create_subkey(r"SOFTWARE\Microsoft\Speech\Voices")?;
    key.set_value(
        "DefaultTokenId",
        &format!(r"HKEY_CURRENT_USER\{SAPI_TOKENS}\{token}"),
    )?;
    println!("  default classic-SAPI voice: {token}");

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if hklm.open_subkey(format!(r"{ONECORE_TOKENS}\{token}")).is_ok() {
        let (key, _) = hkcu.create_subkey(r"SOFTWARE\Microsoft\Speech_OneCore\Voices")?;
        key.set_value(
            "DefaultTokenId",
            &format!(r"HKEY_LOCAL_MACHINE\{ONECORE_TOKENS}\{token}"),
        )?;
        println!("  default OneCore voice (Chrome): {token}");
    } else {
        println!("  no machine-wide token; skipping OneCore default (Chrome needs the elevated install step)");
    }
    Ok(())
}
