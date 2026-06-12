//! Kokoro inference pipeline: text → espeak-ng phonemes → token ids → ONNX → f32 PCM.

use std::path::Path;
use std::sync::Once;

use anyhow::{anyhow, Context, Result};
use ort::session::Session;
use ort::value::Tensor;

use crate::g2p::Espeak;
use crate::logger::elog;
use crate::tokenizer::Tokenizer;
use crate::voices::VoiceBank;

pub const SAMPLE_RATE: u32 = 24000;

/// Kokoro's context limit: 510 phoneme tokens + 2 padding zeros.
const MAX_TOKENS: usize = 510;

static ORT_INIT: Once = Once::new();

pub struct Synth {
    session: Session,
    tokenizer: Tokenizer,
    voices: VoiceBank,
    g2p: Espeak,
    in_ids: String,
    in_style: String,
    in_speed: String,
}

impl Synth {
    pub fn new(assets: &Path) -> Result<Self> {
        ORT_INIT.call_once(|| {
            let dll = assets.join("onnxruntime.dll");
            std::env::set_var("ORT_DYLIB_PATH", &dll);
            // Telemetry off by intent. Note this does NOT prevent the ETW
            // deadlock in ORT < 1.26 (provider registration happens anyway);
            // the real fix is the 1.26+ pin in download-assets.ps1.
            let _ = ort::init().with_telemetry(false).commit();
        });

        let model = assets.join("model.onnx");
        let session = Session::builder()
            .context("ort session builder")?
            .commit_from_file(&model)
            .with_context(|| format!("loading {}", model.display()))?;

        // Input names differ between Kokoro ONNX exports (input_ids/tokens).
        let names: Vec<String> = session.inputs().iter().map(|i| i.name().to_string()).collect();
        let pick = |needle: &str, fallback: &str| -> String {
            names
                .iter()
                .find(|n| n.contains(needle))
                .cloned()
                .unwrap_or_else(|| fallback.to_string())
        };
        let in_ids = if names.iter().any(|n| n.contains("id") || n.contains("token")) {
            names
                .iter()
                .find(|n| n.contains("id") || n.contains("token"))
                .unwrap()
                .clone()
        } else {
            "input_ids".to_string()
        };
        let in_style = pick("style", "style");
        let in_speed = pick("speed", "speed");
        elog!("model inputs: {names:?} -> ids={in_ids} style={in_style} speed={in_speed}");

        Ok(Self {
            session,
            tokenizer: Tokenizer::from_file(&assets.join("tokenizer.json"))?,
            voices: VoiceBank::new(assets.join("voices")),
            g2p: Espeak::new(assets)?,
            in_ids,
            in_style,
            in_speed,
        })
    }

    pub fn synthesize(&mut self, text: &str, voice: &str, speed: f32) -> Result<Vec<f32>> {
        let phonemes = self.g2p.phonemize(text)?;
        let mut ids = self.tokenizer.encode(&phonemes);
        if ids.is_empty() {
            return Err(anyhow!("no tokens for text: {text:?} (phonemes: {phonemes:?})"));
        }
        ids.truncate(MAX_TOKENS);

        let style = self.voices.style(voice, ids.len())?;

        let mut input: Vec<i64> = Vec::with_capacity(ids.len() + 2);
        input.push(0);
        input.extend_from_slice(&ids);
        input.push(0);
        let n = input.len();

        let outputs = self.session.run(ort::inputs![
            self.in_ids.as_str() => Tensor::from_array(([1usize, n], input))?,
            self.in_style.as_str() => Tensor::from_array(([1usize, style.len()], style))?,
            self.in_speed.as_str() => Tensor::from_array(([1usize], vec![speed]))?,
        ])?;

        let (_, data) = outputs[0].try_extract_tensor::<f32>()?;
        Ok(data.to_vec())
    }
}
