use crate::SharedMidiState;
use fundsp::audiounit::AudioUnit;
use fundsp::math::{clamp01, xerp};
use fundsp::net::Net;
use fundsp::prelude64::{adsr_live, envelope2, moog_q};
use fundsp::shared::Shared;

/// Pipes a pitch into `synth`, then modulates the output volume depending on MIDI status.
pub fn simple_sound(state: &SharedMidiState, synth: Box<dyn AudioUnit>) -> Box<dyn AudioUnit> {
    let control = state.control_var();
    state.assemble_unpitched_sound(
        synth,
        Box::new(control >> envelope2(move |_, n| clamp01(n))),
    )
}

#[derive(Clone)]
/// Represents ADSR (Attack/Decay/Sustain/Release) settings for the purpose of generating MIDI-ready sounds.
pub struct Adsr {
    pub attack: Shared,
    pub decay: Shared,
    pub sustain: Shared,
    pub release: Shared,
}
impl Default for Adsr {
    fn default() -> Self {
        Self {
            attack: Shared::new(0.01),
            decay: Shared::new(0.3),
            sustain: Shared::new(0.6),
            release: Shared::new(0.5),
        }
    }
}

impl Adsr {
    pub fn configure(&self, attack: f32, decay: f32, sustain: f32, release: f32) {
        self.attack.set_value(attack);
        self.decay.set_value(decay);
        self.sustain.set_value(sustain);
        self.release.set_value(release);
    }
}
