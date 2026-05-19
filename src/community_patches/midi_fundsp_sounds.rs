use crate::patch_builder::{CcMap, SoundEntry};
use crate::patch_helpers::Adsr;
use crate::{SharedMidiState, SynthFunc, register_sound};
use fundsp::audiounit::AudioUnit;
use fundsp::prelude64::*;
use std::collections::HashMap;
use std::sync::Arc;

fn basic_pluck() -> Box<dyn AudioUnit> {
    Box::new((square() & saw()) >> lowpass_hz(3000.0, 0.5))
}

//todo: make this into a general synth: pro style...waveshaper with 2 shapes, 3 shapes, 2, with some detune control,
// todo: this should be an engine with 2 oscilators with independent levels (pulse width modulation too?), detune and pitch shit of 1 octave up and down
pub fn saw_square_soft(params: &SquareSawSoftParams, cc: &CcMap) -> SynthFunc {
    Arc::new(Box::new(
        (move |state: &SharedMidiState| {
            state.assemble_unpitched_sound(basic_pluck(), state.boxed_adsr())
        }),
    ))
}

register_sound!(
    name: "Square_saw_soft",    // display name & base for struct name
    factory: saw_square_soft,
    construction_params: [(volume, 0.8)], // parameter name + default value
    cc_params: [("brightness", 1, 0.5)]   // CC param: name, default knob index, default value
);
