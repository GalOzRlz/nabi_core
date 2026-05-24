// use crate::instruments::{dirty_guitar, hit_comb_pipe, pluck_comb_string};
// use crate::patch_builder::*;
// use crate::patch_helpers::Adsr;
// use crate::{register_sound, SharedMidiState};
// use fundsp::prelude::{lowpass_hz, shape, AudioUnit};
// use fundsp::prelude64::{constant, sine_hz, Atan};
// use fundsp::shape::Tanh;
//
//
// pub fn harpsichord(state: &SharedMidiState) -> Box<dyn AudioUnit> {
//     state.adsr.configure(
//         0.005,
//         0.8,
//         0.0,
//         0.0,
//     );
//     let gate = state.control_var().clone();
//     let mix = (state.bent_pitch().clone() | gate | constant(0.0))
//         >> pluck_comb_string()
//         >> lowpass_hz(9000.0, 0.5);
//     state.assemble_pitched_sound(Box::new(mix), state.boxed_adsr())
// }
//
// pub fn plastic_pipe(state: &SharedMidiState) -> Box<dyn AudioUnit> {
//     let adsr = state.adsr.clone();
//     adsr.attack.set_value(12.3);
//     let gate = state.control_var().clone();
//     let mix = (state.bent_pitch().clone() | gate | constant(0.0))
//         >> hit_comb_pipe() * 5.0
//         >> shape(Tanh(1.0))
//         >> lowpass_hz(7000.0, 0.5);
//     state.assemble_pitched_sound(Box::new(mix), state.boxed_adsr())
// }
//
// pub fn chorused_dirty_guitar(state: &SharedMidiState) -> Box<dyn AudioUnit> {
//     state.adsr.configure(
//          0.005,
//          0.8,
//          1.0,
//          0.5,
//     );
//     let base_pitch = state.bent_pitch();
//     let lfo1 = sine_hz(3.0) * 0.0065;
//     let pitch1 = base_pitch.clone() * (constant(1.0) + lfo1);
//     let gate = state.control_var();
//     let dg = dirty_guitar();
//     state.assemble_pitched_sound(Box::new(dg(pitch1, gate.clone()) * 6.6 >> shape(Atan(5.0))), state.boxed_adsr())
// }
//
// register_sound!("chorused_dirty_guitar", chorused_dirty_guitar);
// register_sound!("plastic_pipe", plastic_pipe);

use crate::common_definitions::params::{ParamInfo, Parameterized};
use crate::sound_engine::params::TwoOscMorphParams;
use crate::sound_engine::sound_building::SoundBuilder;
use crate::sound_engine::sound_building::SoundEntry;
use crate::{SharedMidiState, register_sound};
use fundsp::audiounit::AudioUnit;
use fundsp::prelude64::*;

// todo: make this into morph2: 2 osc with custom morphing and leveling - 2 morph cc 2 volume cc for each osc
pub fn saw_to_square(_params: &TwoOscMorphParams, state: &SharedMidiState) -> Box<dyn AudioUnit> {
    let b_cc = state.get_sound_cc_or(1, 0.5);
    let synth = Box::new(
        (square() * (constant(1.0) - b_cc.clone()) & saw() * b_cc) * 2.0
            >> lowpass_hz(10000.0, 0.5),
    );
    state.assemble_unpitched_sound(synth, state.boxed_adsr())
}

register_sound!(
    name: "Square_saw_soft",
    params: TwoOscMorphParams,
    factory: saw_to_square,
    cc_params: [("balance", 1)]
);

//todo: add a general synth: pro6 style...2 oscillators with shapes cascading (saw, trianle, pulse) - detune control,
// todo: this should be an engine with 2 oscilators with independent levels (pulse width modulation too?), detune and pitch shit of 1 octave up and down
