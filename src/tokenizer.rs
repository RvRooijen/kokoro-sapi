//! Phoneme → token id mapping, loaded from the HF tokenizer.json that ships
//! with onnx-community/Kokoro-82M-v1.0-ONNX (model.vocab maps each phoneme
//! character to an id).

use std::collections::HashMap;
use std::path::Path;

use anyhow::{anyhow, Context, Result};

pub struct Tokenizer {
    vocab: HashMap<char, i64>,
}

impl Tokenizer {
    pub fn from_file(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let json: serde_json::Value = serde_json::from_str(&raw)?;
        let vocab_obj = json
            .pointer("/model/vocab")
            .and_then(|v| v.as_object())
            .ok_or_else(|| anyhow!("no model.vocab in {}", path.display()))?;

        let mut vocab = HashMap::new();
        for (k, v) in vocab_obj {
            let (Some(c), Some(id)) = (single_char(k), v.as_i64()) else {
                continue;
            };
            vocab.insert(c, id);
        }
        if vocab.is_empty() {
            return Err(anyhow!("empty vocab in {}", path.display()));
        }
        Ok(Self { vocab })
    }

    /// Unknown phonemes are dropped (espeak emits a few symbols outside
    /// Kokoro's vocab, e.g. ties and stress variants it wasn't trained on).
    pub fn encode(&self, phonemes: &str) -> Vec<i64> {
        phonemes
            .chars()
            .filter_map(|c| self.vocab.get(&c).copied())
            .collect()
    }
}

fn single_char(s: &str) -> Option<char> {
    let mut it = s.chars();
    let c = it.next()?;
    it.next().is_none().then_some(c)
}
