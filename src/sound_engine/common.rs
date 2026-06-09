use fundsp::Frame;
use fundsp::audionode::Map;
use fundsp::combinator::An;
use fundsp::prelude::AudioUnit;
use fundsp::prelude64::{U1, map, semitone_ratio};

/// Generic mapping for cc values (0.0-1.0) resulting in frequency ratios matching the desired detuning.
/// Used as a multiplier with the base frequency provided by the patch tuner.
pub fn detune_map(semitone: f32) -> Box<dyn AudioUnit> {
    Box::new(map(move |i: &Frame<f32, U1>| {
        let semitones = -semitone + 2.0 * semitone * i[0]; // 0→-semitone, 1→+semitone
        semitone_ratio(semitones)
    }))
}

/// Detune mapping for cc values (0.0-1.0) between -1 semitones and +1 semitones.
/// Used as a multiplier with the base frequency provided by the patch tuner.
///
pub fn detune_map_semitone() -> An<Map<fn(&Frame<f32, U1>) -> f32, U1, f32>> {
    map(move |i: &Frame<f32, U1>| {
        let semitones = -1.0 + 2.0 * i[0];
        semitone_ratio(semitones)
    })
}
