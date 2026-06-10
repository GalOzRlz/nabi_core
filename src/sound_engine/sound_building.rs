use crate::SharedMidiState;
use crate::common::params::{CcArray, CcInit, Parameterized};
use crate::config_builder::ConfigurableMappings;
use fundsp::audiounit::AudioUnit;
use linkme::distributed_slice;
use std::collections::HashMap;
use std::sync::Arc;

#[distributed_slice]
pub static SOUNDS: [SoundFactory] = [..];

type SoundRegistry = HashMap<&'static str, &'static SoundFactory>;

pub fn get_sounds_registry() -> SoundRegistry {
    let registry: SoundRegistry = SOUNDS.iter().map(|e| (e.params.name, e)).collect();
    registry
}

pub fn get_sound_from_registry(sound_name: &str) -> &'static SoundFactory {
    get_sounds_registry()
        .get(sound_name)
        .unwrap_or_else(|| panic!("Unknown sound: {}", sound_name))
}

pub type SoundBuilder = fn(state: &SharedMidiState, params: &Parameterized) -> Box<dyn AudioUnit>;

#[derive(Clone)]
pub struct SoundFactory {
    pub builder: SoundBuilder,
    pub params: Parameterized,
}

/// `SynthFunc` objects translate `SharedMidiState` values into [fundsp](https://crates.io/crates/fundsp) audio graphs.
pub type SynthFunc = Arc<dyn Fn(&SharedMidiState) -> Box<dyn AudioUnit> + Send + Sync>;

impl CcInit for SoundFactory {
    fn get_initial_cc(&self) -> CcArray {
        self.params.get_initial_cc()
    }
}

impl SoundFactory {
    pub fn process_config(&mut self, config: Option<&ConfigurableMappings>) {
        let Some(sound_toml_config) = config else {
            return;
        };
        let mut new_params = self.params.clone();
        new_params.apply_toml_overrides(sound_toml_config);
        self.params = new_params;
    }

    pub fn build_synth(&self) -> SynthFunc {
        let function = self.builder.clone();
        let config = self.params.clone();
        Arc::new(move |state: &SharedMidiState| -> Box<dyn AudioUnit> {
            let mut unit = function(state, &config);
            unit.allocate();
            unit
        })
    }
}
