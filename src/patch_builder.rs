use crate::common_definitions::params::ParamInfo;
use crate::effects::effects_building::FxChainFactory;
use crate::tunings::TunerBuilder;
use crate::{SharedMidiState, SynthFactory};
use fundsp::prelude64::AudioUnit;
use inventory;
use std::collections::HashMap;
use toml;
use toml::Table;

pub type CcMap = HashMap<String, usize>;
// ---- Knob labels (shared with effects_builders) ----
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnobGroup {
    Sound,
    Effect,
}

#[derive(Debug, Clone)]
pub struct KnobLabel {
    pub group: KnobGroup,
    pub index: usize, // 1‑based logical knob
    pub label: String,
}

// ---- Sound builder signature ----
pub type SoundBuilder = fn(state: &SharedMidiState, config: &Table) -> Box<dyn AudioUnit>;

// ---- Sound registry ----
pub struct SoundEntry {
    pub name: &'static str,
    pub builder: SoundBuilder,
    pub param_info: fn() -> &'static [ParamInfo],
    pub cc_params: &'static [(&'static str, usize)],
}

inventory::collect!(SoundEntry);

// ---- Registration macro (name: only) ----
#[macro_export]
macro_rules! register_sound {
    (
        name: $name:expr,
        params: $params_type:ty,
        factory: $factory_fn:ident,
        cc_params: [ $( ($cc_name:expr, $cc_default_knob:expr) ),* $(,)? ]
    ) => {
        inventory::submit! {
            SoundEntry {
                name: $name,
                builder: (|state: &$crate::SharedMidiState,
                           config: &toml::Table|
                 -> Box<dyn AudioUnit> {
                    let params = <$params_type as Parameterized>::from_table(config);
                    $factory_fn(&params, state)
                }) as SoundBuilder,
                param_info: <$params_type as Parameterized>::param_info as fn() -> &'static [ParamInfo],
                cc_params: &[ $( ($cc_name, $cc_default_knob) ),* ],
            }
        }
    };
}

#[derive(Clone)]
pub struct PatchDef {
    pub sound_factory: SynthFactory,
    pub name: String,
    pub tuning: TunerBuilder,
    pub effects: FxChainFactory,
}

// ---- PatchTable ----
pub const NUM_PATCH_SLOTS: usize = 2_usize.pow(7);

#[derive(Clone)]
pub struct PatchTable {
    pub entries: Vec<PatchDef>,
}

impl PatchTable {
    pub fn new(entries: Vec<PatchDef>) -> Self {
        Self { entries }
    }
}

// pub fn new_sound(sound: Box<dyn AudioUnit>, shared_midi_state: SharedMidiState) -> SynthFunc {
//     Arc::new(Box::new((move |state: &SharedMidiState| { state.assemble_unpitched_sound(sound, state.boxed_adsr())
//     })))
// }
