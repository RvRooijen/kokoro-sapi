//! Smoke test through native SAPI — the same path Chrome takes. Opens the
//! per-user token directly by id, since SAPI never enumerates HKCU tokens.

use anyhow::{Context, Result};
use windows::core::PCWSTR;
use windows::Win32::Media::Speech::{ISpObjectToken, ISpVoice, SpObjectToken, SpVoice};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
};

pub fn speak_test(token: &str) -> Result<()> {
    let token_id: Vec<u16> = format!(
        r"HKEY_CURRENT_USER\SOFTWARE\Microsoft\Speech\Voices\Tokens\{token}"
    )
    .encode_utf16()
    .chain(std::iter::once(0))
    .collect();
    let text: Vec<u16> =
        "Hello! This is Kokoro speaking through SAPI. If you can hear this, the bridge works."
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let voice: ISpVoice =
            CoCreateInstance(&SpVoice, None, CLSCTX_ALL).context("creating SpVoice")?;
        let tok: ISpObjectToken =
            CoCreateInstance(&SpObjectToken, None, CLSCTX_ALL).context("creating SpObjectToken")?;
        tok.SetId(PCWSTR::null(), PCWSTR(token_id.as_ptr()), false)
            .context("voice token not found - run `setup install` first")?;
        voice.SetVoice(&tok).context("selecting voice")?;
        voice
            .Speak(PCWSTR(text.as_ptr()), 0, None)
            .context("speaking (check engine.log in the assets dir)")?;
    }
    println!("  spoke with {token}");
    Ok(())
}
