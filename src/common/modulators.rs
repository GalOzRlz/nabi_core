use crate::common::adapters::StaticParamsAudioNodeAdapter;
use crate::common::fundsp::to_net;
use crate::common::helpers::to_mono_unit;
use fundsp::Frame;
use fundsp::audionode::Map;
use fundsp::audiounit::AudioUnit;
use fundsp::audiounit::Unit;
use fundsp::funutd::math::Float;
use fundsp::math::{SegmentInterpolator, ease_noise, spline_noise};
use fundsp::prelude64::{An, Net, U0, U1, U2, follow, lfo, map, semitone_ratio, sine_hz, unit};
use std::sync::Arc;

fn handle_bipolar(lfo: Net, change_to_unipolar: bool) -> Net {
    if change_to_unipolar {
        to_unipolar(lfo)
    } else {
        lfo
    }
}

fn to_unipolar(lfo: Net) -> Net {
    (lfo * 0.5) + 0.5
}

/// Accepts a raw oscillator function and return an lfo function that accepts frequency, phase and unipolar (true/false) settings
pub fn lfo_builder(osc: Box<dyn AudioUnit>, to_unipolar: bool) -> Net {
    let osc = to_net(unit::<U0, U1>(osc));
    handle_bipolar(osc, to_unipolar)
}

pub fn smooth_random_lfo_freq(freq: f32) -> Net {
    to_net(lfo(move |t| spline_noise(1, t * freq as f64)) >> follow(freq - 0.05))
}

pub fn smooth_random_lfo() -> An<StaticParamsAudioNodeAdapter<1, 1>> {
    let mut node = An(StaticParamsAudioNodeAdapter::<1, 1>::new(Arc::new(
        |args: [f32; 1]| smooth_random_lfo_freq(args[0]),
    )));
    node.disable_fadeout();
    node
}

pub fn smooth_noise_constructor<T: Float + fundsp::Float>(
    smoothing_func: impl SegmentInterpolator<T> + Send + Sync + 'static,
    freq: T,
) -> Net {
    let freq_f64 = fundsp::Float::to_f64(freq);
    let node = lfo(move |t| {
        let x = t * freq_f64;
        let result: T = ease_noise(smoothing_func.clone(), 1, <T as fundsp::Num>::from_f64(x));
        fundsp::Float::to_f64(result)
    });
    to_net(node)
}

fn sine_lfo(freq: f32, phase: f32, unipolar: bool) -> Net {
    let raw = to_net(sine_hz(freq).phase(phase));
    handle_bipolar(raw, unipolar)
}

/// Generic mapping for cc values (0.0-1.0) resulting in frequency ratios matching the desired detuning.
/// Used as a multiplier with a base frequency.
pub fn detune_map(semitone: f32) -> An<Unit<U1, U1>> {
    let mapping = Box::new(map(move |i: &Frame<f32, U1>| {
        let semitones = -semitone + 2.0 * semitone * i[0];
        semitone_ratio(semitones)
    }));
    to_mono_unit(mapping)
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

/// Receives 0.0-1.0 values and outputs a step to use in unidirectional spreading.
/// Maximum Hertz signifies the positive pole limit. E.g. 50 hz means a range from -50hz to +50hz.
/// Step count signifies the amount of steps between negative pole and positive pole.
pub(crate) fn cc_unidirectional_spread_step(max_hz: f32, step_count: usize) -> An<Unit<U1, U1>> {
    let mapper = Box::new(map(move |cc_net: &Frame<f32, U1>| {
        (cc_net[0] * max_hz * 2.0) / step_count as f32
    }));
    to_mono_unit(mapper)
}

pub fn cc_to_cents_by_step(step_count: usize, current_step: usize) -> An<Unit<U2, U1>> {
    assert!(current_step < step_count, "current_step is out of range!");
    let mapper = Box::new(map(move |cc_net: &Frame<f32, U2>| {
        let ratio = semitone_ratio(cc_net[1]);
        let base = cc_net[0];
        let spread_factor = 2.0 * (current_step as f32 / (step_count - 1) as f32) - 1.0;
        ((ratio * base - base) * spread_factor) + base
    }));
    unit::<U2, U1>(mapper)
}
