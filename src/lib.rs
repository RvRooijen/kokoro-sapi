//! Kokoro TTS as a SAPI5 voice.
//!
//! This cdylib is an in-process COM server implementing `ISpTTSEngine`.
//! Registration is per-user (HKCU) via `scripts/register.ps1` — no admin needed.

#![allow(non_snake_case)]

mod config;
mod engine;
mod g2p;
mod logger;
mod synth;
mod tokenizer;
mod voices;

use std::ffi::c_void;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;

use engine::KokoroEngine;

/// CLSID of the engine; must match the GUID in scripts/register.ps1.
pub const CLSID_KOKORO_ENGINE: GUID = GUID::from_u128(0x6a2c7f52_3b19_4e5d_9c01_8f4a2d7b61e3);

#[implement(IClassFactory)]
struct ClassFactory;

impl IClassFactory_Impl for ClassFactory_Impl {
    fn CreateInstance(
        &self,
        punkouter: Ref<IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut c_void,
    ) -> Result<()> {
        if !punkouter.is_null() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let engine: IUnknown = KokoroEngine::new().into();
        unsafe { engine.query(riid, ppvobject).ok() }
    }

    fn LockServer(&self, _flock: BOOL) -> Result<()> {
        Ok(())
    }
}

#[no_mangle]
extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    unsafe {
        if rclsid.is_null() || riid.is_null() || ppv.is_null() {
            return E_POINTER;
        }
        if *rclsid != CLSID_KOKORO_ENGINE {
            return CLASS_E_CLASSNOTAVAILABLE;
        }
        let factory: IClassFactory = ClassFactory.into();
        factory.query(riid, ppv)
    }
}

#[no_mangle]
extern "system" fn DllCanUnloadNow() -> HRESULT {
    // The DLL stays loaded for the lifetime of the host process; SAPI hosts
    // (Chrome, System.Speech) unload it with the process.
    S_FALSE
}
