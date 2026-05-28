use crate::SharedMidiState;
use crate::common_definitions::params::Parameterized;
use crate::config_builder::TomlSoundConfigSection;
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

pub fn get_sound_from_registry(
    sound_name: &str,
    registry: &SoundRegistry,
) -> &'static SoundFactory {
    registry
        .get(sound_name)
        .unwrap_or_else(|| panic!("Unknown effect: {}", sound_name))
}

pub type SoundBuilder = fn(state: &SharedMidiState, config: &Parameterized) -> Box<dyn AudioUnit>;

#[derive(Clone)]
pub struct SoundFactory {
    pub builder: SoundBuilder,
    pub params: Parameterized,
}

/// `SynthFunc` objects translate `SharedMidiState` values into [fundsp](https://crates.io/crates/fundsp) audio graphs.
pub type SynthFunc = Arc<dyn Fn(&SharedMidiState) -> Box<dyn AudioUnit> + Send + Sync>;

// impl SoundFactory {
//     pub(crate) fn get_initial_cc(&self) -> _ {
//         todo!()
//     }
// }

impl SoundFactory {
    pub fn process_toml(&self, config: Option<&TomlSoundConfigSection>) {
        let Some(config) = config else { return };
        let mut runtime_params = self.params.clone();
    }

    pub fn build(&self, state: &SharedMidiState) -> SynthFunc {
        let function = self.builder.clone();
        let config = self.params.clone();
        Arc::new(move |state: &SharedMidiState| -> Box<dyn AudioUnit> { function(state, &config) })
    }
}
