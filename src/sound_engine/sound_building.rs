use crate::SharedMidiState;
use crate::common_definitions::params::ParamInfo;
use crate::patch_builder::{KnobGroup, KnobLabel};
use fundsp::audiounit::AudioUnit;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::sync::Arc;
use toml::Table;

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

/// `SynthFunc` objects translate `SharedMidiState` values into [fundsp](https://crates.io/crates/fundsp) audio graphs.
pub type SynthFunc = Arc<dyn Fn(&SharedMidiState) -> Box<dyn AudioUnit> + Send + Sync>;

pub trait FromTable: Sized {
    fn from_table(table: &Table) -> Self;
}

impl<T> FromTable for T
where
    T: DeserializeOwned + Default,
{
    fn from_table(table: &Table) -> Self {
        let value: toml::Value = table.clone().into();
        T::deserialize(value).unwrap_or_default()
    }
}

#[derive(Clone)]
pub struct SoundFactory {
    pub builder: SoundBuilder,
    pub knob_labels: Vec<KnobLabel>,
    pub config: Table,
    pub initial_cc: Vec<f32>,
}

impl SoundFactory {
    pub fn new(builder_func_name: &str, config: Table, sound_cc_count: usize) -> Self {
        let registry: HashMap<&str, &SoundEntry> = inventory::iter::<SoundEntry>()
            .map(|e| (e.name, e))
            .collect();
        let entry = registry
            .get(builder_func_name)
            .expect("synth with stated function name doesn't exist!");
        let builder = entry.builder.to_owned();
        let mut knob_labels = Vec::new();
        let mut knob_map = HashMap::new();
        for (param_name, default_knob) in entry.cc_params.iter() {
            let mut knob = *default_knob;

            // Clamp or should we ignore?
            if knob < 1 {
                knob = 1;
            }
            if knob > sound_cc_count {
                knob = sound_cc_count;
            }

            knob_map.insert(param_name.to_string(), knob);

            knob_labels.push(KnobLabel {
                group: KnobGroup::Sound,
                index: knob,
                label: format!("{}: {}", param_name, param_name),
            })
        }
        let mut lables = knob_labels.clone();
        let mut initial_knobs = vec![0.0_f32; sound_cc_count.max(1)];
        for v in config.iter() {
            let c_label = v.0;
            for l in lables.iter_mut() {
                if l.label == *c_label {
                    // cc is 1-...
                    initial_knobs[l.index - 1] = v.1.as_float().expect("illegal value!") as f32
                }
            }
        }
        Self {
            builder,
            knob_labels,
            config: config.clone(),
            initial_cc: initial_knobs,
        }
    }

    pub fn build(&self) -> SynthFunc {
        let function = self.builder.clone();
        let config = self.config.clone();
        Arc::new(move |state: &SharedMidiState| -> Box<dyn AudioUnit> { function(state, &config) })
    }
}
