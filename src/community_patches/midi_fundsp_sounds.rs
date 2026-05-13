use crate::patch_builders::Adsr;
use crate::patch_builders::PatchEntry;
use crate::{register_sound, SharedMidiState};
use fundsp::audiounit::AudioUnit;
use fundsp::prelude64::*;

fn basic_pluck() -> Box<dyn AudioUnit> {
    Box::new((square() & saw()) >> lowpass_hz(3000.0, 0.5))
}

pub fn clavichord_soft(state: &SharedMidiState) -> Box<dyn AudioUnit> {
    let adsr = Adsr {
        attack: 0.01,
        decay: 0.2,
        sustain: 0.1,
        release: 0.5,
    };
    state.assemble_unpitched_sound(basic_pluck(), adsr.boxed(state))
}

register_sound!("clavichord_soft", clavichord_soft);