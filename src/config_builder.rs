pub const ENCODER_COUNT: usize = 4;

/// Determines the voice stealing strategy:
/// LegatoOldest: Keep envelope and steal the oldest voice
/// LegatoLast: either oldest or latest voice
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum VoiceStealingConfig {
    LegatoOldest,
    LegatoLast,
}

/// Determine if voices are freed from current voices queue by instrument ADSR or by being at zero volume.
/// Release on zero is a bit costlier but allows for 0.0 release sounds to play better.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum FreeVoiceStrategy {
    FollowADSR,
    ReleaseOnZero,
}

/// Configuration block for extra features
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub voice_stealing: VoiceStealingConfig,
    pub voice_release: FreeVoiceStrategy,
    pub cc_mappings: Vec<usize>,
    pub cc_default_values: Vec<f32>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            voice_stealing: VoiceStealingConfig::LegatoOldest,
            voice_release: FreeVoiceStrategy::ReleaseOnZero,
            // todo: make these overrideable by controller.toml
            cc_mappings: vec![74, 71, 76, 77],
            cc_default_values: vec![0.0, 0.0, 0.0, 1.0],
        }
    }
}

// to do: implement toml overrides for cc mapping and then per-sound defaults

use std::collections::HashMap;
use serde::Deserialize;
use crate::sound_builders::cc_array::CcArray;
use crate::sound_builders::{IntoSpeakerDef, ProgramTable, SoundBuilder, SoundEntry};

#[derive(Deserialize)]
struct TomlProgram {
    function: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    cc: CcArray,
}

#[derive(Deserialize)]
struct ProgramFile {
    program: Vec<TomlProgram>,
}

// ---------------------------------------------------------------------------
// Loading & merging
// ---------------------------------------------------------------------------

fn load_program_file(path: &str) -> Result<Vec<TomlProgram>, Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(path)?;
    let file: ProgramFile = toml::from_str(&text)?;
    Ok(file.program)
}

/// Load multiple TOML files, merge duplicates (last definition wins for CC and name).
fn load_all_programs(paths: &[&str]) -> Vec<TomlProgram> {
    let mut merged: Vec<TomlProgram> = Vec::new();
    let mut index_map: HashMap<String, usize> = HashMap::new(); // function name -> index

    for path in paths {
        let programs = match load_program_file(path) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Skipping {}: {}", path, e);
                continue;
            }
        };
        for mut prog in programs {
            if let Some(&idx) = index_map.get(&prog.function) {
                // Override CCs and name (if given)
                merged[idx].cc = prog.cc;
                if prog.name.is_some() {
                    merged[idx].name = prog.name;
                }
            } else {
                if prog.name.is_none() {
                    prog.name = Some(prog.function.clone());
                }
                index_map.insert(prog.function.clone(), merged.len());
                merged.push(prog);
            }
        }
    }
    merged
}

fn build_program_table(programs: &[TomlProgram]) -> ProgramTable {
    // Lookup from name to raw builder pointer
    let builder_map: HashMap<&str, SoundBuilder> = inventory::iter::<SoundEntry>()
        .map(|e| (e.name, e.builder))
        .collect();

    let mut entries = Vec::new();
    for prog in programs {
        let builder = match builder_map.get(prog.function.as_str()) {
            Some(&b) => b,
            None => {
                eprintln!(
                    "Unknown function '{}' for program '{}', skipping",
                    prog.function,
                    prog.name.as_deref().unwrap_or(&prog.function)
                );
                continue;
            }
        };
        let cc = prog.cc.0; // [u8; 4]
        let name = prog.name.clone().unwrap_or_else(|| prog.function.clone());
        let def = (name, builder.into_speaker_def(), cc);
        entries.push(def);
    }
    ProgramTable::new(entries)
}
