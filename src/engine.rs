//! The SAPI5 TTS engine object: ISpTTSEngine + ISpObjectWithToken.
//!
//! SAPI calls `SetObjectToken` right after creation (the token is the registry
//! voice entry, carrying our config), then `GetOutputFormat` and `Speak`.

use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Mutex;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Media::Audio::{WAVEFORMATEX, WAVE_FORMAT_PCM};
use windows::Win32::Media::Speech::*;
use windows::Win32::System::Com::{CoTaskMemAlloc, CoTaskMemFree};

use crate::logger::elog;
use crate::synth::{Synth, SAMPLE_RATE};

/// SPDFID_WaveFormatEx from sapi.h (not exposed by the windows crate).
const SPDFID_WAVEFORMATEX: GUID = GUID::from_u128(0xc31adbae_527f_4ff5_a230_f62bb61ff70c);

const BYTES_PER_MS: u32 = SAMPLE_RATE * 2 / 1000;

// SPVESACTIONS flags returned by ISpTTSEngineSite::GetActions.
const SPVES_ABORT: u32 = 1;
#[allow(dead_code)]
const SPVES_SKIP: u32 = 2;
const SPVES_RATE: u32 = 4;
const SPVES_VOLUME: u32 = 8;

struct EngineState {
    token: Option<ISpObjectToken>,
    voice: String,
    synth: Option<Synth>,
    synth_failed: bool,
}

// Safety: SAPI may call the engine from different threads (ThreadingModel=Both),
// but serializes calls per voice instance; the Mutex makes this explicit.
unsafe impl Send for EngineState {}

#[implement(ISpTTSEngine, ISpObjectWithToken)]
pub struct KokoroEngine {
    state: Mutex<EngineState>,
}

impl KokoroEngine {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(EngineState {
                token: None,
                voice: "af_heart".into(),
                synth: None,
                synth_failed: false,
            }),
        }
    }
}

fn token_string(token: &ISpObjectToken, name: &str) -> Option<String> {
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let pwstr = token.GetStringValue(PCWSTR(wide.as_ptr())).ok()?;
        if pwstr.is_null() {
            return None;
        }
        let s = pwstr.to_string().ok();
        CoTaskMemFree(Some(pwstr.as_ptr() as *const c_void));
        s
    }
}

impl ISpObjectWithToken_Impl for KokoroEngine_Impl {
    fn SetObjectToken(&self, ptoken: Ref<ISpObjectToken>) -> Result<()> {
        let token = ptoken.ok()?.clone();
        let assets = crate::config::assets_dir(token_string(&token, "AssetsDir"));
        crate::logger::set_dir(&assets);
        let mut state = self.state.lock().unwrap();
        if let Some(v) = token_string(&token, "VoiceName") {
            state.voice = v;
        }
        elog!("SetObjectToken: voice={} assets={}", state.voice, assets.display());
        state.token = Some(token);
        Ok(())
    }

    fn GetObjectToken(&self) -> Result<ISpObjectToken> {
        self.state
            .lock()
            .unwrap()
            .token
            .clone()
            .ok_or_else(|| Error::from(E_FAIL))
    }
}

impl ISpTTSEngine_Impl for KokoroEngine_Impl {
    fn Speak(
        &self,
        _dwspeakflags: u32,
        _rguidformatid: *const GUID,
        _pwaveformatex: *const WAVEFORMATEX,
        ptextfraglist: *const SPVTEXTFRAG,
        poutputsite: Ref<ISpTTSEngineSite>,
    ) -> Result<()> {
        let site = poutputsite.ok()?.clone();
        let result = catch_unwind(AssertUnwindSafe(|| self.speak_inner(ptextfraglist, &site)));
        match result {
            Ok(r) => r,
            Err(_) => {
                elog!("PANIC in Speak — returning E_FAIL");
                Err(E_FAIL.into())
            }
        }
    }

    fn GetOutputFormat(
        &self,
        _ptargetfmtid: *const GUID,
        _ptargetwaveformatex: *const WAVEFORMATEX,
        poutputformatid: *mut GUID,
        ppcomemoutputwaveformatex: *mut *mut WAVEFORMATEX,
    ) -> Result<()> {
        unsafe {
            if poutputformatid.is_null() || ppcomemoutputwaveformatex.is_null() {
                return Err(E_POINTER.into());
            }
            let wfx = CoTaskMemAlloc(std::mem::size_of::<WAVEFORMATEX>()) as *mut WAVEFORMATEX;
            if wfx.is_null() {
                return Err(E_OUTOFMEMORY.into());
            }
            *wfx = WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_PCM as u16,
                nChannels: 1,
                nSamplesPerSec: SAMPLE_RATE,
                nAvgBytesPerSec: SAMPLE_RATE * 2,
                nBlockAlign: 2,
                wBitsPerSample: 16,
                cbSize: 0,
            };
            *poutputformatid = SPDFID_WAVEFORMATEX;
            *ppcomemoutputwaveformatex = wfx;
        }
        Ok(())
    }
}

/// One text run extracted from the fragment list, with its source offset
/// (UTF-16 units) for word-boundary events.
enum Frag {
    Text { utf16: Vec<u16>, src_offset: u32 },
    SilenceMs(u32),
}

impl KokoroEngine_Impl {
    fn speak_inner(&self, ptextfraglist: *const SPVTEXTFRAG, site: &ISpTTSEngineSite) -> Result<()> {
        let frags = unsafe { collect_frags(ptextfraglist) };
        let mut rate_adj = unsafe { site.GetRate().unwrap_or(0) };
        let mut volume = unsafe { site.GetVolume().unwrap_or(100) };
        let mut bytes_written: u64 = 0;

        let mut state = self.state.lock().unwrap();
        ensure_synth(&mut state);
        let voice = state.voice.clone();
        let mut pacer = Pacer::new();

        for frag in frags {
            match frag {
                Frag::SilenceMs(ms) => {
                    let buf = vec![0u8; (ms * BYTES_PER_MS) as usize & !1];
                    if !write_pcm(site, &buf, &mut bytes_written, &mut pacer) {
                        return Ok(());
                    }
                }
                Frag::Text { utf16, src_offset } => {
                    let text = String::from_utf16_lossy(&utf16);
                    for sentence in split_sentences(&text) {
                        let actions = unsafe { site.GetActions() };
                        if actions & SPVES_ABORT != 0 {
                            elog!("abort requested");
                            return Ok(());
                        }
                        if actions & SPVES_RATE != 0 {
                            rate_adj = unsafe { site.GetRate().unwrap_or(rate_adj) };
                        }
                        if actions & SPVES_VOLUME != 0 {
                            volume = unsafe { site.GetVolume().unwrap_or(volume) };
                        }
                        let speed = 2f32.powf(rate_adj as f32 / 10.0).clamp(0.5, 2.0);

                        let samples = match synthesize(&mut state, &sentence.text, &voice, speed) {
                            Ok(s) => s,
                            Err(e) => {
                                elog!("synthesis failed: {e:#}");
                                continue;
                            }
                        };
                        let pcm = to_pcm16(&samples, volume as f32 / 100.0);

                        emit_word_boundaries(site, &sentence, src_offset, bytes_written, pcm.len());
                        if !write_pcm(site, &pcm, &mut bytes_written, &mut pacer) {
                            return Ok(());
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

/// ~100 ms of audio per Write. Small chunks give us a GetActions check every
/// ~100 ms, and a purge unblocks a pending Write with an error.
const CHUNK_BYTES: usize = (SAMPLE_RATE as usize / 10) * 2;

/// Max audio we keep buffered ahead of real-time playback. ISpVoice::Pause
/// does NOT stop the audio device — it only blocks our Write calls, and
/// everything already buffered plays out (measured: pause took effect seconds
/// late, at the end of the sentence). Since we synthesize much faster than
/// real-time, unthrottled writes buffer the whole sentence and pause feels
/// broken. Pacing writes to playback speed + this lead caps the pause delay.
const MAX_LEAD: std::time::Duration = std::time::Duration::from_millis(250);

/// Tracks wall-clock vs written audio so writes stay ~MAX_LEAD ahead of
/// playback. The clock starts at the first write.
struct Pacer {
    start: Option<std::time::Instant>,
}

impl Pacer {
    fn new() -> Self {
        Self { start: None }
    }

    /// Sleep until `bytes_written` is at most MAX_LEAD ahead of playback.
    /// Returns false on abort.
    fn throttle(&mut self, site: &ISpTTSEngineSite, bytes_written: u64) -> bool {
        let start = self.start.get_or_insert_with(std::time::Instant::now);
        let audio_pos =
            std::time::Duration::from_secs_f64(bytes_written as f64 / (SAMPLE_RATE * 2) as f64);
        loop {
            if unsafe { site.GetActions() } & SPVES_ABORT != 0 {
                elog!("abort requested while pacing");
                return false;
            }
            let elapsed = start.elapsed();
            if audio_pos <= elapsed + MAX_LEAD {
                return true;
            }
            std::thread::sleep((audio_pos - elapsed - MAX_LEAD).min(std::time::Duration::from_millis(50)));
        }
    }

    /// A Write that blocked for long means SAPI held us (pause, or a full
    /// device buffer): playback did not consume audio during that time, so
    /// shift the clock to keep the lead at MAX_LEAD instead of bursting.
    fn on_blocked_write(&mut self, blocked_for: std::time::Duration) {
        if let Some(start) = self.start.as_mut() {
            *start += blocked_for;
        }
    }
}

/// Returns false when speaking should stop (abort/purge) — not an error.
fn write_pcm(site: &ISpTTSEngineSite, data: &[u8], bytes_written: &mut u64, pacer: &mut Pacer) -> bool {
    for chunk in data.chunks(CHUNK_BYTES) {
        if !pacer.throttle(site, *bytes_written) {
            return false;
        }
        if unsafe { site.GetActions() } & SPVES_ABORT != 0 {
            elog!("abort requested mid-write");
            return false;
        }
        let before = std::time::Instant::now();
        if let Err(e) = unsafe { site.Write(chunk.as_ptr() as *const c_void, chunk.len() as u32) } {
            elog!("write interrupted (purge): {e}");
            return false;
        }
        let blocked = before.elapsed();
        if blocked > std::time::Duration::from_millis(250) {
            pacer.on_blocked_write(blocked);
        }
        *bytes_written += chunk.len() as u64;
    }
    true
}

fn ensure_synth(state: &mut EngineState) {
    if state.synth.is_some() || state.synth_failed {
        return;
    }
    let assets = crate::config::assets_dir(None);
    match Synth::new(&assets) {
        Ok(s) => state.synth = Some(s),
        Err(e) => {
            elog!("Synth init failed ({e:#}) — falling back to test tone. Run scripts/download-assets.ps1.");
            state.synth_failed = true;
        }
    }
}

fn synthesize(state: &mut EngineState, text: &str, voice: &str, speed: f32) -> anyhow::Result<Vec<f32>> {
    match state.synth.as_mut() {
        Some(s) => s.synthesize(text, voice, speed),
        // Fallback so the COM/SAPI plumbing can be smoke-tested without assets:
        // a 440 Hz tone, ~80 ms per word.
        None => {
            let words = text.split_whitespace().count().max(1);
            let n = words * 80 * SAMPLE_RATE as usize / 1000;
            Ok((0..n)
                .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / SAMPLE_RATE as f32).sin() * 0.25)
                .collect())
        }
    }
}

unsafe fn collect_frags(mut cur: *const SPVTEXTFRAG) -> Vec<Frag> {
    let mut out = Vec::new();
    while !cur.is_null() {
        let f = &*cur;
        let action = f.State.eAction;
        if action == SPVA_Speak || action == SPVA_Pronounce || action == SPVA_SpellOut {
            if !f.pTextStart.is_null() && f.ulTextLen > 0 {
                let slice = std::slice::from_raw_parts(f.pTextStart.0, f.ulTextLen as usize);
                out.push(Frag::Text {
                    utf16: slice.to_vec(),
                    src_offset: f.ulTextSrcOffset,
                });
            }
        } else if action == SPVA_Silence {
            out.push(Frag::SilenceMs(f.State.SilenceMSecs));
        }
        cur = f.pNext;
    }
    out
}

struct Sentence {
    text: String,
    /// (utf16_offset_in_frag, utf16_len, char_start_in_sentence) per word
    words: Vec<(u32, u32, usize)>,
    char_len: usize,
}

/// Split on sentence-final punctuation, keeping the terminator with the
/// sentence (Kokoro uses it for prosody). Tracks UTF-16 offsets for events.
fn split_sentences(text: &str) -> Vec<Sentence> {
    let mut sentences = Vec::new();
    let mut start_c = 0usize; // char index of current sentence start
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        let is_end = matches!(c, '.' | '!' | '?' | '…' | ';' | ':');
        if is_end || i == chars.len() - 1 {
            let end = i + 1;
            let s: String = chars[start_c..end].iter().collect();
            if s.trim().chars().any(|c| c.is_alphanumeric()) {
                sentences.push(make_sentence(&chars, start_c, end));
            }
            start_c = end;
        }
        i += 1;
    }
    sentences
}

fn make_sentence(chars: &[char], start: usize, end: usize) -> Sentence {
    // utf16 offset of `start` within the fragment
    let utf16_before: usize = chars[..start].iter().map(|c| c.len_utf16()).sum();
    let mut words = Vec::new();
    let mut u16_off = utf16_before;
    let mut j = start;
    while j < end {
        if chars[j].is_whitespace() {
            u16_off += chars[j].len_utf16();
            j += 1;
            continue;
        }
        let word_start_u16 = u16_off;
        let word_start_char = j - start;
        let mut word_u16_len = 0usize;
        while j < end && !chars[j].is_whitespace() {
            word_u16_len += chars[j].len_utf16();
            u16_off += chars[j].len_utf16();
            j += 1;
        }
        words.push((word_start_u16 as u32, word_u16_len as u32, word_start_char));
    }
    Sentence {
        text: chars[start..end].iter().collect::<String>().trim().to_string(),
        words,
        char_len: end - start,
    }
}

/// Queue SPEI_WORD_BOUNDARY events for a sentence. Audio offsets are estimated
/// proportionally over the sentence's PCM bytes — SAPI fires each event when
/// playback reaches its offset, which is what drives word highlighting in
/// Chrome's reading mode and other readers.
fn emit_word_boundaries(
    site: &ISpTTSEngineSite,
    sentence: &Sentence,
    src_offset: u32,
    base_bytes: u64,
    pcm_len: usize,
) {
    if sentence.words.is_empty() || sentence.char_len == 0 {
        return;
    }
    let events: Vec<SPEVENT> = sentence
        .words
        .iter()
        .map(|&(u16_off, u16_len, char_start)| {
            let audio_off = base_bytes + (pcm_len * char_start / sentence.char_len) as u64 & !1;
            let mut ev: SPEVENT = unsafe { std::mem::zeroed() };
            ev._bitfield = SPEI_WORD_BOUNDARY.0; // eEventId:16 | elParamType:16 (UNDEFINED=0)
            ev.ulStreamNum = 0;
            ev.ullAudioStreamOffset = audio_off;
            ev.wParam = WPARAM(u16_len as usize);
            ev.lParam = LPARAM((src_offset + u16_off) as isize);
            ev
        })
        .collect();
    unsafe {
        if let Err(e) = site.AddEvents(events.as_ptr(), events.len() as u32) {
            elog!("AddEvents failed: {e}");
        }
    }
}

fn to_pcm16(samples: &[f32], gain: f32) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let v = (s * gain).clamp(-1.0, 1.0);
        out.extend_from_slice(&((v * i16::MAX as f32) as i16).to_le_bytes());
    }
    out
}
