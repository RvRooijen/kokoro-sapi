# kokoro-sapi

[Kokoro](https://huggingface.co/hexgrad/Kokoro-82M) neural TTS as a **Windows SAPI5 voice** — so it shows up in Chrome's reading mode, Edge, System.Speech and anything else that uses Windows voices. Registration is **per-user (HKCU), no admin required**.

```
text ──SAPI──> kokoro_sapi.dll ──espeak-ng──> IPA phonemes ──onnxruntime──> 24 kHz PCM ──> SAPI
```

Everything runs in-process and offline. The DLL loads `onnxruntime.dll` dynamically (no import-lib linking) and lazily, so voice enumeration stays cheap; the model is only loaded on first speak.

## Build & install

Build on Windows (the DLL is a COM in-process server; MSVC toolchain via [rustup](https://rustup.rs)):

```powershell
cargo build --release
.\scripts\download-assets.ps1      # model + voices + onnxruntime + espeak (~400 MB, or -Quantized for ~150 MB)
.\scripts\register.ps1             # HKCU registration, no admin
```

Smoke test (works in Windows PowerShell and pwsh; uses native SAPI, the same path Chrome takes):

```powershell
.\scripts\test-speak.ps1
```

Tip: you can register before downloading assets — the engine then speaks a 440 Hz test tone, which is a quick way to verify the COM/SAPI plumbing in isolation.

## Chrome reading mode

Chrome builds its system-voice list from **`HKLM\SOFTWARE\Microsoft\Speech_OneCore\Voices`** — hardcoded in [`content/browser/speech/tts_win.cc`](https://source.chromium.org/chromium/chromium/src/+/main:content/browser/speech/tts_win.cc), with classic SAPI only as a fallback category that never triggers in practice. Per-user registration is therefore invisible to Chrome; register machine-wide once from an elevated prompt:

```powershell
.\scripts\register.ps1 -Machine
```

Reading mode's voice menu then still shows only **one "System text-to-speech voice" per language**: its filtering (`read_aloud/tts_voice_filtering.ts`) groups system voices by language and keeps `voice.default || voices[0]`. SAPI enumeration returns the *default token first*, so make Kokoro the OneCore default to win that race for en-US:

```powershell
.\scripts\set-default-voice.ps1     # sets the classic-SAPI and OneCore defaults (per-user)
```

Then restart Chrome fully (check Task Manager — background mode keeps it alive) and select "System text-to-speech voice". For languages where Kokoro is the only system voice (en-GB with Emma on a default Windows install), no default juggling is needed. `speechSynthesis.getVoices()` in the DevTools console shows exactly what Chrome sees; `scripts\unregister.ps1` removes everything from all registries.

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
- `src/synth.rs` — the Kokoro pipeline (phonemes → token ids → style vector → ONNX → f32 samples).
- `src/g2p.rs` — `espeak-ng.dll` via `libloading`. Kokoro's training G2P (misaki) falls back to espeak, so espeak IPA output is in-distribution.
- Config travels through the registry voice token (`VoiceName`, `AssetsDir`); the engine receives it via `SetObjectToken`.
- Diagnostics: `%LOCALAPPDATA%\KokoroSapi\engine.log` (the DLL runs inside the host app, so there's no console).

## Gotchas

- **ONNX Runtime must be 1.26+** (the download script pins this). Older builds, including 1.22, can deadlock inside `CreateEnv`: ORT's Windows telemetry registers an ETW provider, and with the DiagTrack session subscribed (the Windows default) the enable callback fires synchronously and self-deadlocks. There is no runtime opt-out; don't downgrade. `examples/ortload.rs` is a standalone repro/smoke test for this.
- **SAPI never enumerates per-user (HKCU) voice tokens** — neither native `GetVoices()` nor System.Speech lists them. They do work when opened by token id, and classic SAPI apps respect them as default voice via `DefaultTokenId`. But anything that should show the voices in a list — Chrome (OneCore registry), Edge read aloud, reader extensions — needs `register.ps1 -Machine` from an elevated prompt. The COM class stays per-user, so even then it only works for this Windows account.

## Known limitations / roadmap

- **Latency**: synthesis is per-sentence; the first audio arrives after the first sentence is done (~real-time on CPU). Streaming in smaller chunks would improve start latency.
- **Word boundaries are estimated** (proportional to character position). Good enough for highlighting; exact timing would need per-token durations from the model.
- **`SPVES_SKIP` is ignored** — sentence-skip navigation in some readers won't jump.
- **Commas don't influence prosody** (espeak's `TextToPhonemes` drops mid-sentence punctuation; only the sentence terminator is re-appended).
- **SSML/XML tags**: bookmarks, pitch and per-fragment rate changes are ignored.
- Voice blending (Kokoro can mix style vectors) and a Dutch voice via a second backend (e.g. Piper) would fit naturally behind the same engine.
