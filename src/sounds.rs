use crate::instruments::{dirty_guitar, hit_comb_pipe, pluck_comb_string};
use crate::patch_builder::*;
use crate::patch_helpers::Adsr;
use crate::{register_sound, SharedMidiState};
use fundsp::prelude::{lowpass_hz, shape, AudioUnit};
use fundsp::prelude64::{constant, sine_hz, Atan};
use fundsp::shape::Tanh;

/// Returns an on-off Triangle wave.

pub fn harpsichord(state: &SharedMidiState) -> Box<dyn AudioUnit> {
    let adsr = Adsr {
        attack: 0.005,
        decay: 0.8,
        sustain: 0.0,
        release: 0.0,
    };
    let gate = state.control_var().clone();
    let mix = (state.bent_pitch().clone() | gate | constant(0.0))
        >> pluck_comb_string()
        >> lowpass_hz(9000.0, 0.5);
    state.assemble_pitched_sound(Box::new(mix), adsr.boxed(state))
}

pub fn plastic_pipe(state: &SharedMidiState) -> Box<dyn AudioUnit> {
    let adsr = Adsr {
        attack: 0.005,
        decay: 0.5,
        sustain: 0.0,
        release: 0.0,
    };
    let gate = state.control_var().clone();
    let mix = (state.bent_pitch().clone() | gate | constant(0.0))
        >> hit_comb_pipe() * 5.0
        >> shape(Tanh(1.0))
        >> lowpass_hz(7000.0, 0.5);
    state.assemble_pitched_sound(Box::new(mix), adsr.boxed(state))
}

pub fn chorused_dirty_guitar(state: &SharedMidiState) -> Box<dyn AudioUnit> {
    let adsr = Adsr {
        attack: 0.005,
        decay: 0.8,
        sustain: 1.0,
        release: 0.5,
    };
    let base_pitch = state.bent_pitch();
    let lfo1 = sine_hz(3.0) * 0.0065;
    let pitch1 = base_pitch.clone() * (constant(1.0) + lfo1);
    let gate = state.control_var();
    let dg = dirty_guitar();
    state.assemble_pitched_sound(Box::new(dg(pitch1, gate.clone()) * 6.6 >> shape(Atan(5.0))), adsr.boxed(state))
}

register_sound!("chorused_dirty_guitar", chorused_dirty_guitar);
register_sound!("plastic_pipe", plastic_pipe);