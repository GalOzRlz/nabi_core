use crate::effects::to_net;
use fundsp::funutd::math::Float;
use fundsp::math::{ease_noise, spline_noise, SegmentInterpolator};
use fundsp::prelude::WaveSynth;
use fundsp::prelude64::{follow, lfo, sine_hz, An, Constant, Net, Pipe, U1};

fn handle_bipolar(lfo: Net, unipolar: bool) -> Net {
    if unipolar {
        to_unipolar(lfo)
    }
    else {
        lfo
    }
}

fn to_unipolar(lfo: Net) -> Net{
    lfo * 0.5 + 0.5
}

/// Accepts a raw oscillator function and return an lfo function that accepts frequency, phase and unipolar (true/false) settings
pub fn lfo_builder<F>(osc: F) -> impl Fn(f32, f32, bool) -> Net
where
    F: Fn(f32) ->  An<Pipe<Constant<U1>, WaveSynth<U1>>>,
    {
    move |freq: f32, phase: f32, bipolar: bool| {
        let raw = to_net(osc(freq).phase(phase));
        handle_bipolar(raw, bipolar)
    }
    }

pub fn smooth_random_lfo(freq: f64) -> Net {
    to_net(lfo(move |t| spline_noise(1, t * freq)) >> follow(freq as f32 - 0.05))
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

fn sine_lfo(freq: f32, phase:f32, unipolar: bool) -> Net {
    let raw = to_net(sine_hz(freq).phase(phase));
    handle_bipolar(raw, unipolar)
}

