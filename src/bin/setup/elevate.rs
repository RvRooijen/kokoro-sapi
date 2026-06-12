//! UAC self-elevation: re-run setup.exe elevated for the machine-wide registry
//! steps, so the user never has to open an elevated terminal themselves.

use anyhow::{anyhow, bail, Result};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetExitCodeProcess, OpenProcessToken, WaitForSingleObject, INFINITE,
};
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};

pub fn is_elevated() -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut info = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut info as *mut _ as *mut core::ffi::c_void),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok && info.TokenIsElevated != 0
    }
}

/// Run `setup.exe <params>` elevated and wait for it. Returns the exit code.
/// Errors when the user dismisses the UAC prompt.
pub fn run_self_elevated(params: &str) -> Result<u32> {
    if is_elevated() {
        bail!("already elevated; run the subcommand directly");
    }
    let exe = std::env::current_exe()?;
    let exe_w: Vec<u16> = exe
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let params_w: Vec<u16> = params.encode_utf16().chain(std::iter::once(0)).collect();

    let mut sei = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(exe_w.as_ptr()),
        lpParameters: PCWSTR(params_w.as_ptr()),
        nShow: 0, // SW_HIDE: the child only writes registry keys
        ..Default::default()
    };
    unsafe {
        ShellExecuteExW(&mut sei).map_err(|_| anyhow!("UAC prompt was dismissed"))?;
        if sei.hProcess.is_invalid() {
            bail!("no process handle from ShellExecuteEx");
        }
        WaitForSingleObject(sei.hProcess, INFINITE);
        let mut code = 0u32;
        let _ = GetExitCodeProcess(sei.hProcess, &mut code);
        let _ = CloseHandle(sei.hProcess);
        Ok(code)
    }
}
