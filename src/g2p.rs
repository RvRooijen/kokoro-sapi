//! Phonemization via espeak-ng.dll, loaded dynamically so we ship no import lib.
//!
//! Kokoro was trained on IPA phonemes (misaki uses espeak as fallback G2P, so
//! espeak output is in-distribution). espeak_TextToPhonemes drops punctuation;
//! the engine splits per sentence and we re-append the final punctuation mark,
//! which Kokoro uses for prosody.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use libloading::{Library, Symbol};

const ESPEAK_AUDIO_OUTPUT_RETRIEVAL: c_int = 1;
const ESPEAK_CHARS_UTF8: c_int = 1;
const ESPEAK_PHONEMES_IPA: c_int = 0x02;

type FnInitialize = unsafe extern "C" fn(c_int, c_int, *const c_char, c_int) -> c_int;
type FnSetVoiceByName = unsafe extern "C" fn(*const c_char) -> c_int;
type FnTextToPhonemes =
    unsafe extern "C" fn(*mut *const c_void, c_int, c_int) -> *const c_char;

pub struct Espeak {
    // Library must outlive the symbols; declared after them so it drops last.
    text_to_phonemes: Symbol<'static, FnTextToPhonemes>,
    _lib: &'static Library,
}

impl Espeak {
    pub fn new(assets: &Path) -> Result<Self> {
        let dll = assets.join("espeak-ng.dll");
        let lib = unsafe { Library::new(&dll) }
            .with_context(|| format!("loading {}", dll.display()))?;
        // Leak the library: espeak-ng keeps global state and the engine lives
        // for the process lifetime anyway.
        let lib: &'static Library = Box::leak(Box::new(lib));

        unsafe {
            let initialize: Symbol<FnInitialize> = lib.get(b"espeak_Initialize\0")?;
            let set_voice: Symbol<FnSetVoiceByName> = lib.get(b"espeak_SetVoiceByName\0")?;
            let text_to_phonemes: Symbol<'static, FnTextToPhonemes> =
                lib.get(b"espeak_TextToPhonemes\0")?;

            let data_dir = CString::new(assets.to_string_lossy().as_bytes())?;
            let rate = initialize(ESPEAK_AUDIO_OUTPUT_RETRIEVAL, 0, data_dir.as_ptr(), 0);
            if rate < 0 {
                return Err(anyhow!(
                    "espeak_Initialize failed ({rate}); is espeak-ng-data next to espeak-ng.dll?"
                ));
            }
            let voice = CString::new("en-us")?;
            let r = set_voice(voice.as_ptr());
            if r != 0 {
                return Err(anyhow!("espeak_SetVoiceByName(en-us) failed ({r})"));
            }

            Ok(Self {
                text_to_phonemes,
                _lib: lib,
            })
        }
    }

    pub fn phonemize(&self, text: &str) -> Result<String> {
        let trailing_punct = text
            .trim_end()
            .chars()
            .last()
            .filter(|c| matches!(c, '.' | '!' | '?' | '…' | ';' | ':' | ','));

        let c_text = CString::new(text)?;
        let mut out = String::new();
        unsafe {
            let mut ptr: *const c_void = c_text.as_ptr() as *const c_void;
            // espeak returns one clause per call and advances ptr.
            while !ptr.is_null() {
                let res =
                    (self.text_to_phonemes)(&mut ptr, ESPEAK_CHARS_UTF8, ESPEAK_PHONEMES_IPA);
                if res.is_null() {
                    break;
                }
                let clause = CStr::from_ptr(res).to_string_lossy();
                if !out.is_empty() && !clause.is_empty() {
                    out.push(' ');
                }
                out.push_str(clause.trim());
            }
        }
        if let Some(p) = trailing_punct {
            out.push(p);
        }
        Ok(out)
    }
}
