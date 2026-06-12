//! Kokoro voice style vectors. Each <voice>.bin (from the HF onnx repo's
//! voices/ folder) is a raw little-endian f32 array of shape (N, 1, 256);
//! row index = number of phoneme tokens, so prosody scales with input length.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};

const STYLE_DIM: usize = 256;

pub struct VoiceBank {
    dir: PathBuf,
    cache: HashMap<String, Vec<f32>>,
}

impl VoiceBank {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            cache: HashMap::new(),
        }
    }

    pub fn style(&mut self, voice: &str, token_len: usize) -> Result<Vec<f32>> {
        if !self.cache.contains_key(voice) {
            let path = self.dir.join(format!("{voice}.bin"));
            let bytes = std::fs::read(&path)
                .with_context(|| format!("reading voice {}", path.display()))?;
            if bytes.len() % (STYLE_DIM * 4) != 0 {
                return Err(anyhow!(
                    "{}: size {} is not a multiple of {}",
                    path.display(),
                    bytes.len(),
                    STYLE_DIM * 4
                ));
            }
            let floats: Vec<f32> = bytes
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect();
            self.cache.insert(voice.to_string(), floats);
        }

        let data = &self.cache[voice];
        let rows = data.len() / STYLE_DIM;
        let idx = token_len.min(rows - 1);
        Ok(data[idx * STYLE_DIM..(idx + 1) * STYLE_DIM].to_vec())
    }
}
