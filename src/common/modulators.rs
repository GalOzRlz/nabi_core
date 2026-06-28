use crate::common::adapters::StaticParamsAudioNodeAdapter;
use crate::common::fundsp::to_net;
use fundsp::audiounit::AudioUnit;
use fundsp::funutd::math::Float;
use fundsp::math::{SegmentInterpolator, ease_noise, spline_noise};
use fundsp::prelude64::{An, Net, U0, U1, follow, lfo, sine_hz, unit};
use std::sync::Arc;

fn handle_bipolar(lfo: Net, change_to_unipolar: bool) -> Net {
    if change_to_unipolar {
        to_unipolar(lfo)
    } else {
        lfo
    }
}

fn to_unipolar(lfo: Net) -> Net {
    lfo * 0.5 + 0.5
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
