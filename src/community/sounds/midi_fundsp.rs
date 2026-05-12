use crate::sound_builder::{register_sound, SharedMidiState};
use fundsp::prelude64::*;

pub fn piano(state: &SharedMidiState) -> Box<dyn AudioUnit> {
    // Use cc[0] and cc[1] as extra parameters if desired
    let brightness = state.cc[0] as f64 / 127.0;
    let env = adsr_live(0.01, 0.5, 0.3, 0.8, state.gate);
    let osc = sine_hz(state.freq);
    Box::new(osc * env * 0.8)
}

register_sound!("piano", piano);