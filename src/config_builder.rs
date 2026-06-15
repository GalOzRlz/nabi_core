use crate::effects::effects_building::FxChainFactory;
use crate::patch_builder::{PatchDef, PatchTable};
use crate::sound_engine::sound_building::{SoundFactory, get_sound_from_registry};
use crate::tuning::tunings::{TUNERS, TunerBuilder};
use fundsp::math::midi_hz;
use globwalk::GlobWalkerBuilder;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use toml::Value;

// ---------- dynamic knob sizing ----------
pub const MAX_KNOBS_PER_GROUP: usize = 16;

fn default_fx_cc_vals() -> Vec<u8> {
    vec![74, 71, 76, 77]
}
fn default_sound_cc_vals() -> Vec<u8> {
    vec![80, 81, 82, 83]
}

// ---------- voice management enums ----------
#[derive(Debug, Copy, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum VoiceStealingConfig {
    LegatoOldest,
    LegatoLast,
}

#[derive(Debug, Copy, Clone, PartialEq, Deserialize, Serialize)]
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
    pub patches_path: PathBuf,
    pub sound_cc_mapping: Vec<u8>,
    pub fx_cc_mapping: Vec<u8>,
    pub left_right_buttons: [u8; 2],
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            voice_stealing: VoiceStealingConfig::LegatoLast,
            voice_release: FreeVoiceStrategy::FollowADSR,
            sound_cc_mapping: default_sound_cc_vals(),
            fx_cc_mapping: default_fx_cc_vals(),
            patches_path: {
                let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                path.push("patches".to_string());
                path
            },
            left_right_buttons: [25, 26],
        }
    }
}

// ---------- TOML structures for midi.toml ----------
#[derive(Deserialize, Serialize)]
pub struct GlobalConfigToml {
    #[serde(default)]
    pub global: GlobalSection,
}

#[derive(Deserialize, Default, Serialize)]
pub struct GlobalSection {
    #[serde(default)]
    pub sound_cc_mapping: Option<Vec<u8>>,

    #[serde(default)]
    pub fx_cc_mapping: Option<Vec<u8>>,

    #[serde(default)]
    synth_stealing: Option<VoiceStealingConfig>,

    #[serde(default)]
    synth_release: Option<FreeVoiceStrategy>,

    #[serde(default)]
    patches_path: Option<PathBuf>,

    #[serde(default)]
    pub left_right_buttons: Option<[u8; 2]>,
}

pub fn load_global_config(path: &str) -> GlobalConfig {
    let defaults = GlobalConfig::default();

    match fs::read_to_string(path) {
        Ok(text) => match toml::from_str::<GlobalConfigToml>(&text) {
            Ok(cfg) => GlobalConfig {
                sound_cc_mapping: cfg
                    .global
                    .sound_cc_mapping
                    .unwrap_or(defaults.sound_cc_mapping),
                fx_cc_mapping: cfg.global.fx_cc_mapping.unwrap_or(defaults.fx_cc_mapping),
                voice_stealing: cfg.global.synth_stealing.unwrap_or(defaults.voice_stealing),
                voice_release: cfg.global.synth_release.unwrap_or(defaults.voice_release),
                patches_path: cfg
                    .global
                    .patches_path
                    .unwrap_or(defaults.patches_path)
                    .to_path_buf(),
                left_right_buttons: cfg
                    .global
                    .left_right_buttons
                    .unwrap_or(defaults.left_right_buttons),
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
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct TomlPatchDef {
    pub function: String,
    pub name: String,
    pub tuning: Option<String>,
    pub sound: Option<ConfigurableMappings>,
    pub effects: Option<TomlEffectSection>,
}

pub trait ConfigurableMapping {
    fn get_values(&self) -> Option<&HashMap<String, Value>>;
    fn get_mapping(&self) -> Option<&HashMap<String, Value>>;
    fn get_mapping_mut(&mut self) -> Option<&mut HashMap<String, Value>>;
}

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
pub struct ConfigurableMappings {
    pub values: Option<HashMap<String, Value>>,
    pub mapping: Option<HashMap<String, Value>>,
}

impl ConfigurableMapping for ConfigurableMappings {
    fn get_values(&self) -> Option<&HashMap<String, Value>> {
        self.values.as_ref()
    }

    fn get_mapping(&self) -> Option<&HashMap<String, Value>> {
        self.mapping.as_ref()
    }

    fn get_mapping_mut(&mut self) -> Option<&mut HashMap<String, Value>> {
        self.mapping.as_mut()
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct TomlEffectSection {
    pub chain: Option<Vec<String>>,
    pub configs: Option<HashMap<String, ConfigurableMappings>>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct ProgramsFile {
    pub(crate) program: Vec<TomlPatchDef>,
}

impl ProgramsFile {
    pub fn new(program: Vec<TomlPatchDef>) -> Self {
        ProgramsFile { program }
    }
}

#[derive(Debug, Deserialize)]
struct TomlOrderConfig {
    patch_order: Vec<String>,
}

fn load_patch_file(path_buf: &PathBuf) -> Result<Vec<TomlPatchDef>, Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(path_buf)?;
    let file: ProgramsFile = toml::from_str(&text)?;
    Ok(file.program)
}

pub fn load_all_programs(paths: Vec<PathBuf>) -> Vec<TomlPatchDef> {
    let mut all_programs = Vec::new();
    let mut used_names: HashSet<String> = HashSet::new();

    for path in paths {
        let programs = match load_patch_file(&path) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Skipping {:?}: {}", path, e);
                continue;
            }
        };
        for prog in programs {
            let display_name = prog.name.clone();

            if used_names.contains(&display_name) {
                panic!(
                    "Duplicate program name '{}' found in file {:?}. \
                     Each program must have a unique display name.",
                    display_name, path
                );
            }
            used_names.insert(display_name.clone());

            all_programs.push(TomlPatchDef {
                function: prog.function,
                name: display_name,
                tuning: prog.tuning,
                sound: prog.sound,
                effects: prog.effects,
            });
        }
    }
    all_programs
}

// ---------- build the PatchTable ----------
pub fn build_patch_table(programs: &[TomlPatchDef]) -> PatchTable {
    let tuner_map: HashMap<&str, TunerBuilder> =
        TUNERS.into_iter().map(|e| (e.name, e.tuner)).collect();

    let default_tuner = midi_hz;
    let mut patch_defs = Vec::new();

    for prog in programs.iter() {
        // --- resolve tuner ---
        let tuning = if let Some(ref tuning_name) = prog.tuning {
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

        let mut effects = FxChainFactory::new();
        effects.process_config(prog.effects.as_ref());
        let mut sound_factory: SoundFactory =
            get_sound_from_registry(prog.function.as_str()).clone();
        sound_factory.process_config(prog.sound.as_ref());
        let patch_def = PatchDef {
            sound_factory,
            tuning,
            effects,
            toml: prog.clone(),
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
        name_to_entry.insert(entry.toml.name.clone(), (idx, entry));
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
pub fn gather_toml_files_recursive(root: &PathBuf) -> Vec<PathBuf> {
    GlobWalkerBuilder::new(root, "**/*.toml")
        .build()
        .unwrap()
        .filter_map(|entry| entry.ok()) // skip I/O errors
        .filter(|entry| entry.file_type().is_file()) // only regular files
        .filter_map(|entry| fs::canonicalize(entry.path()).ok()) // absolute + existence
        .collect()
}

pub fn create_ordered_patch_table(patch_paths: Vec<PathBuf>, order_path: &str) -> PatchTable {
    let all_programs = load_all_programs(patch_paths);
    let mut patch_table = build_patch_table(&all_programs);

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
