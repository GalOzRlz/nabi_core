use crate::SharedMidiState;
use crate::SynthFunc;
use crate::effects_builders::PatchFxChain;
use crate::tunings::TunerBuilder;
use fundsp::prelude::{AudioUnit, U2, multipass};
use inventory;
use std::collections::HashMap;
use std::sync::Arc;
use toml;

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
pub type SoundBuilder =
    fn(state: &SharedMidiState, config: &toml::Table, cc_map: &CcMap) -> Box<dyn AudioUnit>;

// ---- Sound registry ----
pub struct SoundEntry {
    pub name: &'static str,
    pub builder: SoundBuilder,
    pub construction_defaults: &'static [(&'static str, f64)],
    pub cc_params: &'static [(&'static str, usize, f64)],
}
inventory::collect!(SoundEntry);

// ---- Registration macro (name: only) ----
#[macro_export]
macro_rules! register_sound {
    (
        name: $name:expr,
        factory: $factory_fn:ident,
        construction_params: [ $( ($c_name:ident, $c_default:expr) ),* $(,)? ],
        cc_params: [ $( ($cc_name:expr, $cc_default_knob:expr, $cc_default_val:expr) ),* $(,)? ]
    ) => {
        paste::paste! {
            // ----- generated params struct (unchanged) -----
            pub struct [<$name:camel Params>] {
                $( pub $c_name: f64, )*
            }

            impl [<$name:camel Params>] {
                fn from_table(table: &toml::Table) -> Self {
                    Self {
                        $(
                            $c_name: table.get(stringify!($c_name))
                                .and_then(|v| v.as_float())
                                .unwrap_or($c_default),
                        )*
                    }
                }
            }

            // ----- wrapper now matches SoundBuilder signature -----
            fn [<__sound_wrapper_ $name:snake>] (
                state: &$crate::SharedMidiState,
                construction: &toml::Table,
                cc_map: &CcMap,
            ) -> Box<dyn fundsp::prelude64::AudioUnit> {
                let params = [<$name:camel Params>]::from_table(construction);
                // Call the user’s factory, which returns a SynthFunc,
                // then immediately invoke that closure with the state.
                let synth_func = $factory_fn(&params, cc_map);
                synth_func(state)
            }

            // ----- inventory submission (correct cast now) -----
            inventory::submit! {
                $crate::patch_builder::SoundEntry {
                    name: $name,
                    builder: [<__sound_wrapper_ $name:snake>] as $crate::patch_builder::SoundBuilder,
                    construction_defaults: &[ $( (stringify!($c_name), $c_default) ),* ],
                    cc_params: &[ $( ($cc_name, $cc_default_knob, $cc_default_val) ),* ],
                }
            }
        }
    };
}

// ---- PatchDef ----
#[derive(Clone)]
pub struct PatchDef {
    pub function: SynthFunc,
    pub name: String,
    pub tuning: TunerBuilder,
    pub effects: PatchFxChain,
    pub knob_labels: Vec<KnobLabel>,
}

pub type PatchTableItem = PatchDef;

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
