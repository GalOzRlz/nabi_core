use fundsp::shared::Shared;

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

    pub fn new(attack: f32, decay: f32, sustain: f32, release: f32) -> Self {
        let s = Adsr::default();
        s.configure(attack, decay, sustain, release);
        s
    }
}
