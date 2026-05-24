use crate::effects::effects_building::FxChainFactory;
use crate::patch_builder::{PatchDef, PatchTable};
use crate::sound_engine::sound_building::SoundFactory;
use crate::tunings::{TunerBuilder, TunerEntry};
use fundsp::math::midi_hz;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

// ---------- dynamic knob sizing ----------
pub const MAX_KNOBS_PER_GROUP: usize = 16;

fn default_fx_cc_vals() -> Vec<u8> {
    vec![74, 71, 76, 77]
}
fn default_sound_cc_vals() -> Vec<u8> {
    vec![80, 81, 82, 83]
}

// ---------- voice management enums ----------
#[derive(Debug, Copy, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum VoiceStealingConfig {
    LegatoOldest,
    LegatoLast,
}

#[derive(Debug, Copy, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum FreeVoiceStrategy {
    FollowADSR,
    ReleaseOnZero,
}

// ---------- runtime global configuration ----------
#[derive(Debug, Clone, PartialEq)]
pub struct GlobalConfig {
    pub voice_stealing: VoiceStealingConfig,
    pub voice_release: FreeVoiceStrategy,

    pub sound_cc_mapping: Vec<u8>,
    pub fx_cc_mapping: Vec<u8>,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            voice_stealing: VoiceStealingConfig::LegatoLast,
            voice_release: FreeVoiceStrategy::FollowADSR,
            sound_cc_mapping: default_sound_cc_vals(),
            fx_cc_mapping: default_fx_cc_vals(),
        }
    }
}

// ---------- TOML structures for midi.toml ----------
#[derive(Deserialize)]
struct GlobalConfigToml {
    #[serde(default)]
    global: GlobalSection,
}

#[derive(Deserialize, Default)]
struct GlobalSection {
    #[serde(default)]
    sound_cc_mapping: Option<Vec<u8>>,

    #[serde(default)]
    fx_cc_mapping: Option<Vec<u8>>,

    #[serde(default)]
    synth_stealing: Option<VoiceStealingConfig>,

    #[serde(default)]
    synth_release: Option<FreeVoiceStrategy>,
}

pub fn load_global_config(path: &str) -> GlobalConfig {
    let defaults = GlobalConfig::default();

    match std::fs::read_to_string(path) {
        Ok(text) => match toml::from_str::<GlobalConfigToml>(&text) {
            Ok(cfg) => GlobalConfig {
                sound_cc_mapping: cfg
                    .global
                    .sound_cc_mapping
                    .unwrap_or(defaults.sound_cc_mapping),
                fx_cc_mapping: cfg.global.fx_cc_mapping.unwrap_or(defaults.fx_cc_mapping),
                voice_stealing: cfg.global.synth_stealing.unwrap_or(defaults.voice_stealing),
                voice_release: cfg.global.synth_release.unwrap_or(defaults.voice_release),
            },
            Err(e) => {
                eprintln!("Warning: failed to parse midi.toml: {e}. Using defaults.");
                defaults
            }
        },
        Err(_) => {
            eprintln!("midi.toml not found, using default config.");
            defaults
        }
    }
}

// ---------- program TOML structures ----------
#[derive(Deserialize, Clone)]
pub struct TomlPatchDef {
    pub function: String,
    pub name: String,
    pub tuning: Option<String>,
    pub config: Option<toml::Table>,
    pub effects: Option<TomlEffectSection>,
}

#[derive(Deserialize, Clone)]
pub struct TomlEffectSection {
    pub chain: Vec<String>,
    #[serde(flatten)]
    pub extras: HashMap<String, toml::Value>,
}

#[derive(Deserialize)]
struct ProgramsFile {
    program: Vec<TomlPatchDef>,
}

#[derive(Debug, Deserialize)]
struct TomlOrderConfig {
    patch_order: Vec<String>,
}

fn load_patch_file(path: &str) -> Result<Vec<TomlPatchDef>, Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(path)?;
    let file: ProgramsFile = toml::from_str(&text)?;
    Ok(file.program)
}

pub fn load_all_programs(paths: &[&str]) -> Vec<TomlPatchDef> {
    let mut all_programs = Vec::new();
    let mut used_names: HashSet<String> = HashSet::new();

    for path in paths {
        let programs = match load_patch_file(path) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Skipping {}: {}", path, e);
                continue;
            }
        };
        for prog in programs {
            let display_name = prog.name.clone();

            if used_names.contains(&display_name) {
                panic!(
                    "Duplicate program name '{}' found in file {}. \
                     Each program must have a unique display name.",
                    display_name, path
                );
            }
            used_names.insert(display_name.clone());

            all_programs.push(TomlPatchDef {
                function: prog.function,
                name: display_name,
                tuning: prog.tuning,
                config: prog.config,
                effects: prog.effects,
            });
        }
    }
    all_programs
}

// ---------- build the PatchTable ----------
pub fn build_patch_table(programs: &[TomlPatchDef], global_config: &GlobalConfig) -> PatchTable {
    let tuner_map: HashMap<&str, TunerBuilder> = inventory::iter::<TunerEntry>()
        .map(|e| (e.name, e.tuner))
        .collect();

    let default_tuner = midi_hz;
    let effect_cc_count = global_config.fx_cc_mapping.len().max(1);
    let sound_cc_count = global_config.sound_cc_mapping.len().max(1);
    let mut patch_defs = Vec::new();

    for prog in programs {
        // --- resolve tuner ---
        let tuner = if let Some(ref tuning_name) = prog.tuning {
            tuner_map
                .get(tuning_name.as_str())
                .copied()
                .unwrap_or_else(|| {
                    eprintln!("Unknown tuning '{}', using default", tuning_name);
                    default_tuner
                })
        } else {
            default_tuner
        };

        // --- build effect chain ---
        let fx_chain = FxChainFactory::new(prog.effects.as_ref(), effect_cc_count);

        // --- assemble PatchDef ---
        let patch_def = PatchDef {
            sound_factory: SoundFactory::new(
                prog.function.as_str(),
                prog.config.clone().unwrap_or_default(),
                sound_cc_count,
            ),
            name: prog.name.clone(),
            tuning: tuner,
            effects: fx_chain,
        };

        patch_defs.push(patch_def);
    }

    PatchTable::new(patch_defs)
}
// ---------- ordering ----------
fn reorder_by_names(entries: &mut Vec<PatchDef>, order: &[String]) {
    let old_entries = std::mem::take(entries);
    let indexed: Vec<(usize, PatchDef)> = old_entries.into_iter().enumerate().collect();
    let mut name_to_entry: HashMap<String, (usize, PatchDef)> = HashMap::new();
    for (idx, entry) in indexed {
        name_to_entry.insert(entry.name.clone(), (idx, entry));
    }

    let mut new_entries = Vec::with_capacity(name_to_entry.len());
    let mut used_indices = HashSet::new();

    for name in order {
        if let Some((idx, entry)) = name_to_entry.remove(name) {
            new_entries.push(entry);
            used_indices.insert(idx);
        }
    }

    let mut remaining: Vec<(usize, PatchDef)> = name_to_entry.into_values().collect();
    remaining.sort_by_key(|(idx, _)| *idx);
    for (_, entry) in remaining {
        new_entries.push(entry);
    }

    *entries = new_entries;
}

pub fn create_ordered_patch_table(
    patch_paths: &[&str],
    order_path: &str,
    global_config: &GlobalConfig,
) -> PatchTable {
    let all_programs = load_all_programs(patch_paths);
    let mut patch_table = build_patch_table(&all_programs, global_config);

    if let Ok(text) = std::fs::read_to_string(order_path) {
        if let Ok(ord_config) = toml::from_str::<TomlOrderConfig>(&text) {
            eprintln!("Loaded ordered patch table:{:?}", ord_config.patch_order);
            reorder_by_names(&mut patch_table.entries, &ord_config.patch_order);
        } else {
            eprintln!("Failed to parse order.toml inside toml, using default order");
        }
    } else {
        eprintln!("Failed to parse order.toml in read_to_string, using default order");
    }
    patch_table
}
