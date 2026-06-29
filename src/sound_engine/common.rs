use crate::common::helpers::to_mono_unit;
use fundsp::Frame;
use fundsp::audionode::Map;
use fundsp::audiounit::Unit;
use fundsp::combinator::An;
use fundsp::prelude64::{U1, map, semitone_ratio};

/// Generic mapping for cc values (0.0-1.0) resulting in frequency ratios matching the desired detuning.
/// Used as a multiplier with the base frequency provided by the patch tuner.
pub(crate) fn detune_map(semitone: f32) -> An<Unit<U1, U1>> {
    let mapping = Box::new(map(move |i: &Frame<f32, U1>| {
        let semitones = -semitone + 2.0 * semitone * i[0];
        semitone_ratio(semitones)
    }));
    to_mono_unit(mapping)
}

/// Detune mapping for cc values (0.0-1.0) between -1 semitones and +1 semitones.
/// Used as a multiplier with the base frequency provided by the patch tuner.
///
pub(crate) fn detune_map_semitone() -> An<Map<fn(&Frame<f32, U1>) -> f32, U1, f32>> {
    map(move |i: &Frame<f32, U1>| {
        let semitones = -1.0 + 2.0 * i[0];
        semitone_ratio(semitones)
    })
}

/// Receives 0.0-1.0 values and outputs a step to use in unidirectional spreading.
/// Maximum Hertz signifies the positive pole limit. E.g. 50 hz means a range from -50hz to +50hz.
/// Step count signifies the amount of steps between negative pole and positive pole.
pub(crate) fn cc_unidirectional_spread_step(max_hz: f32, step_count: f32) -> An<Unit<U1, U1>> {
    let mapper = Box::new(map(move |cc_net: &Frame<f32, U1>| {
        (cc_net[0] * max_hz * 2.0) / step_count.round()
    }));
    to_mono_unit(mapper)
}
