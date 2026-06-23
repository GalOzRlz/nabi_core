use crate::common::helpers::to_mono_unit;
use crate::common::params::{CcAudioNode, NonCcParam};
use crate::effects::modulators::{smooth_noise_constructor, smooth_random_lfo};
use fundsp::audiounit::Unit;
use fundsp::combinator::An;
use fundsp::math::smooth3;
use fundsp::net::Net;
use fundsp::prelude64::{U1, dc, delay, feedback, lfo, pass, tap, tap_linear};
use std::f32::consts::LN_2;
use std::f64::consts::PI;

/// Pitch shifter inspired by Bitwigs's pitch shifter
pub fn pitch_shifter(pitch_st: &NonCcParam, freq_hz: &NonCcParam) -> An<Unit<U1, U1>> {
    let pitch_st = pitch_st.value.as_f32().unwrap().clamp(-12.0, 12.0);
    let freq_hz = freq_hz.value.as_f32().unwrap().clamp(5.0, 100.0);

    let max_delay = 0.1; //
    let freq_hz = freq_hz.clamp(20.0, 100.0);
    let ratio = (pitch_st * LN_2 * 1.0).exp(); // 2.0 for +12, 0.5 for -12
    let depth_mag = ((ratio - 1.0).abs() / freq_hz).min(max_delay * 0.999);
    let min_delay = max_delay - depth_mag;

    let phasor = lfo(move |t| {
        let phase = (t * freq_hz as f64).fract();
        phase
    });

    let up_delay = dc(max_delay) - (phasor.clone() * dc(depth_mag));
    let down_delay = dc(min_delay) + (phasor * dc(depth_mag));

    // Select which delay to use based on polarity of pitch_st
    let control = dc(if pitch_st >= 0.0 { 1.0 } else { 0.0 });
    let mod_sig = up_delay * control.clone() + down_delay * (dc(1.0) - control);

    // create a small fade in and out for each grain
    let window_env = lfo(move |t: f64| {
        let phase = (t * freq_hz as f64).fract();
        0.5 - 0.5 * (2.0 * PI * phase).cos()
    });

    // Apply the modulated delay line
    let shifted = (pass() | mod_sig) >> tap(min_delay, max_delay);

    let shifted_env = shifted * window_env;

    // Smooth with short decay delay
    let feedback_line = feedback(delay(0.003) * 0.5);

    let wet = Box::new(shifted_env >> feedback_line);
    to_mono_unit(wet)
}

pub fn tape_wow(depth: CcAudioNode) -> Net {
    let wow_ms_range = 0.025;
    let flutter_ms_range = 0.0022;
    let center = 0.030;
    let wow_mod = smooth_random_lfo(0.6);
    let flutter_mod = smooth_noise_constructor(smooth3, 9.0);
    let total_wow = (wow_mod * depth.clone() + 2.0) * wow_ms_range;
    let total_flutter = (flutter_mod * depth + 2.0) * flutter_ms_range;
    let wet_amount = (pass() | total_wow + total_flutter)
        >> tap_linear(
            center - wow_ms_range - flutter_ms_range,
            center + wow_ms_range + flutter_ms_range,
        );
    wet_amount.clone() | wet_amount
}
