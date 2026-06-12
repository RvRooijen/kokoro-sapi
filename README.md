# kokoro-sapi

[Kokoro](https://huggingface.co/hexgrad/Kokoro-82M) neural TTS as a **Windows SAPI5 voice** — a natural-sounding, fully offline voice for Chrome's reading mode, Edge read aloud, and anything else that speaks through Windows voices.

```
text ──SAPI──> kokoro_sapi.dll ──espeak-ng──> IPA phonemes ──onnxruntime──> 24 kHz PCM ──> SAPI
```

Everything runs locally and in-process: no cloud, no server, no Python. Inference is CPU-only (the model is 82M parameters and synthesizes faster than real-time); expect ~400–500 MB of RAM in the host app while a voice is in use, or about a third of that with the quantized model.

## Setup

You need Windows 10/11 (64-bit) and [Rust](https://rustup.rs) with the default MSVC toolchain (rustup offers to install the Visual Studio Build Tools if they're missing).

From a PowerShell in the repo root:

```powershell
cargo build --release
.\scripts\download-assets.ps1      # model + voices + onnxruntime + espeak (~400 MB, or -Quantized for ~150 MB)
.\scripts\register.ps1             # per-user (HKCU), no admin needed
.\scripts\test-speak.ps1           # you should hear Kokoro through your speakers
```

If PowerShell refuses to run the scripts, allow them for this session first: `Set-ExecutionPolicy -Scope Process Bypass`.

For **Chrome's reading mode**, two extra steps are needed ([why](#chrome-reading-mode) below):

```powershell
.\scripts\register.ps1 -Machine    # from an elevated PowerShell
.\scripts\set-default-voice.ps1    # make Kokoro the default system voice
```

Then restart Chrome fully (check Task Manager — background mode keeps it alive), open reading mode and select **"System text-to-speech voice"**. Word highlighting tracks the speech.

`scripts\unregister.ps1` removes all registrations (run elevated if you used `-Machine`); the assets in `%LOCALAPPDATA%\KokoroSapi` can simply be deleted.

Tip: you can register before downloading assets — the engine then speaks a 440 Hz test tone, which is a quick way to verify the COM/SAPI plumbing in isolation.

## Chrome reading mode

Why the extra steps: Chrome builds its system-voice list from **`HKLM\SOFTWARE\Microsoft\Speech_OneCore\Voices`** — hardcoded in [`content/browser/speech/tts_win.cc`](https://source.chromium.org/chromium/chromium/src/+/main:content/browser/speech/tts_win.cc), with classic SAPI only as a fallback category that never triggers in practice. Per-user registration is therefore invisible to Chrome; `register.ps1 -Machine` writes the voice tokens to the OneCore registry (and HKLM SAPI) as well.

Reading mode's voice menu then still shows only **one "System text-to-speech voice" per language**: its filtering (`read_aloud/tts_voice_filtering.ts`) groups system voices by language and keeps `voice.default || voices[0]`. SAPI enumeration returns the *default token first*, which is why `set-default-voice.ps1` makes Kokoro the OneCore default — that wins the en-US slot. For languages where Kokoro is the only system voice (en-GB with Emma on a default Windows install), no default juggling is needed.

`speechSynthesis.getVoices()` in the DevTools console shows exactly what Chrome sees.

## Voices

| SAPI name | Kokoro voice | |
|---|---|---|
| Kokoro Heart (en-US) | `af_heart` | female, the flagship voice |
| Kokoro Bella (en-US) | `af_bella` | female |
| Kokoro Michael (en-US) | `am_michael` | male |
| Kokoro Emma (en-GB) | `bf_emma` | female |

Add more by extending the voice list in `scripts/register.ps1` and the download list in `scripts/download-assets.ps1` ([all voices](https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/tree/main/voices)).

## How it works

- `src/lib.rs` — COM server exports (`DllGetClassObject`) + class factory.
- `src/engine.rs` — `ISpTTSEngine`/`ISpObjectWithToken`. Walks the SAPI fragment list, splits sentences, applies rate (−10..10 → 0.5×..2×) and volume from the site, writes PCM, and queues `SPEI_WORD_BOUNDARY` events with audio offsets estimated proportionally per word — that's what drives word highlighting in readers.
- Writes are paced to real-time plus a ~250 ms lead: `ISpVoice::Pause` doesn't stop the audio device, it only blocks the engine's writes while everything already buffered plays out — so the lead is effectively the pause delay. Abort/purge (Chrome's stop, and its play-after-pause) is honored within ~100 ms.
- `src/synth.rs` — the Kokoro pipeline (phonemes → token ids → style vector → ONNX → f32 samples).
- `src/g2p.rs` — `espeak-ng.dll` via `libloading`. Kokoro's training G2P (misaki) falls back to espeak, so espeak IPA output is in-distribution.
- Config travels through the registry voice token (`VoiceName`, `AssetsDir`); the engine receives it via `SetObjectToken`.
- Diagnostics: `%LOCALAPPDATA%\KokoroSapi\engine.log` (the DLL runs inside the host app, so there's no console).

## Gotchas

- **ONNX Runtime must be 1.26+** (the download script pins this). Older builds, including 1.22, can deadlock inside `CreateEnv`: ORT's Windows telemetry registers an ETW provider, and with the DiagTrack session subscribed (the Windows default) the enable callback fires synchronously and self-deadlocks. There is no runtime opt-out; don't downgrade. `examples/ortload.rs` is a standalone repro/smoke test for this.
- **SAPI never enumerates per-user (HKCU) voice tokens** — neither native `GetVoices()` nor System.Speech lists them. They do work when opened by token id, and classic SAPI apps respect them as default voice via `DefaultTokenId`. But anything that should show the voices in a list — Chrome (OneCore registry), Edge read aloud, reader extensions — needs `register.ps1 -Machine` from an elevated prompt. The COM class stays per-user, so even then it only works for this Windows account.

## Known limitations / roadmap

- **Latency**: synthesis is per-sentence; the first audio arrives after the first sentence is done (~real-time on CPU). Streaming in smaller chunks would improve start latency.
- **Word boundaries are estimated** (proportional to character position). Good enough for highlighting; exact timing would need per-token durations from the model. Note Chrome's reading mode *resumes from the last boundary* after a pause (it purges and re-speaks the remainder rather than calling resume), so boundary drift shows up as a repeated/skipped word on resume.
- **`SPVES_SKIP` is ignored** — sentence-skip navigation in some readers won't jump.
- **Commas don't influence prosody** (espeak's `TextToPhonemes` drops mid-sentence punctuation; only the sentence terminator is re-appended).
- **SSML/XML tags**: bookmarks, pitch and per-fragment rate changes are ignored.
- Voice blending (Kokoro can mix style vectors) and a Dutch voice via a second backend (e.g. Piper) would fit naturally behind the same engine.
