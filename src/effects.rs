use crate::modulators::{smooth_noise_constructor, smooth_random_lfo};
use crate::SharedMidiState;
use fundsp::combinator::An;
use fundsp::prelude64::*;
use std::f32::consts::{LN_2};
use std::f64::consts::PI;
use inventory::Node;

pub fn to_net<F:AudioNode + 'static>(fx: An<F>) -> Net {
    Net::wrap(Box::new(fx))
}

pub fn mono_to_stereo(net: Net) -> Net {
    net.clone() | net
}

pub fn master_limiter() -> Net {
    let block = dcblock() >> limiter(0.002, 0.3);
    let master = multipass::<U2>() >> (block.clone() | block);
    to_net(master)
}

pub(crate) fn cc_smooth() -> An<Follow<f64>> {
    follow(0.005)
}

fn sensitive_cc_smooth() -> An<Follow<f64>> {
    follow(0.15)
}

/// Factory for stereo effects with wet/dry control via Net  (suitable for live Midi CC)
fn cc_controlled_wet_dry_fx(effect: Net, cc_idx: usize, shared_midi_state: &SharedMidiState) -> Net {
    let wet_amount: Net = Net::wrap(Box::new(
        var(&shared_midi_state.control_change[cc_idx].clone())
    ));
    // Duplicate wet to stereo (0 inputs, 2 outputs)
    let wet_amount =  wet_amount >> cc_smooth();
    let wet_stereo = wet_amount.clone() | wet_amount.clone();

    let dry_mono = constant(1.0) - wet_amount;
    let dry_stereo = dry_mono.clone() | dry_mono;

    let pass = Net::wrap(Box::new(multipass::<U2>())); // U2 -> U2 identity
    (pass * dry_stereo) & (effect * wet_stereo)
}

pub fn prophet_lowpass_filter() -> Net {
    Net::wrap(Box::new(
    !butterpass() >> butterpass()))
}


pub fn master_highpass(cc_idx: usize, shared_midi_state: &SharedMidiState, q: f32) -> Net {
    let cutoff_val = var(&shared_midi_state.control_change[cc_idx].clone()) >> cc_smooth();
    let cutoff_hrz = product(constant(8_000.0), cutoff_val) >> cc_smooth();
    Net::wrap(Box::new(
        (pass() | cutoff_hrz >> follow(0.05_f32)) >> highpass_q(q),
    ))
}

pub fn master_reverb(global_fx_cc_idx_1: usize, shared_midi_state: &SharedMidiState) -> Net {
    let reverb_amount: Net = Net::wrap(Box::new(
        var(&shared_midi_state.control_change[global_fx_cc_idx_1].clone())
    ));
    let reverb = to_net(reverb_stereo(10.0, 3.0, 0.4));
    cc_controlled_wet_dry_fx(reverb_amount, global_fx_cc_idx_1, shared_midi_state)
}

pub fn tape_wow(depth: Net) -> Net {
    let wow_ms_range = 0.025;
    let flutter_ms_range = 0.0022;
    let center = 0.030;
    let wow_mod = smooth_random_lfo(0.6);
    let flutter_mod = smooth_noise_constructor(smooth3, 9.0);
    let total_wow = (wow_mod * depth.clone() + 2.0) * wow_ms_range;
    let total_flutter = (flutter_mod * depth + 2.0) * flutter_ms_range;
    let min_delay = center-wow_ms_range-flutter_ms_range;
    let max_delay = center+wow_ms_range+flutter_ms_range;
    let mix = (pass() | total_wow + total_flutter)  >> tap_linear(min_delay, max_delay);
    Net::wrap(Box::new(mix.clone()|mix))
}

pub fn master_tape_effect(cc: usize, shared_midi_state: &SharedMidiState) -> Net {
    let depth: Net = Net::wrap(Box::new(
        var(&shared_midi_state.control_change[cc].clone()))) >> sensitive_cc_smooth();
    tape_wow(depth)
}

pub fn pitch_shifter(pitch_st: f32, freq_hz: f32, wet_amt: f32) -> Net {
    let max_delay = 0.1; // 100 ms – supports grain rates down to ~10 Hz for octave shifts
    let freq_hz= freq_hz.clamp(20.0, 100.0);
    let ratio = (pitch_st * LN_2 * 1.0).exp();   // 2.0 for +12, 0.5 for -12
    let depth_mag = ((ratio - 1.0).abs() / freq_hz)
        .min(max_delay  * 0.999);
    let min_delay = max_delay - depth_mag;

    // Rising sawtooth LFO (0 → 1) – same for both directions
    let phasor = lfo(move |t: f64| {
        let phase = (t * freq_hz as f64).fract();
        phase
    });

    // Two candidate delay modulations:
    // - up_delay: decreasing delay → pitch up
    // - down_delay: increasing delay → pitch down
    let up_delay = dc(max_delay) - (phasor.clone() * dc(depth_mag ));
    let down_delay = dc(min_delay) + (phasor * dc(depth_mag ));

    // Select which delay to use based on sign of pitch_st
    let control = dc(if pitch_st >= 0.0 { 1.0 } else { 0.0 });
    let mod_sig = up_delay * control.clone() + down_delay * (dc(1.0) - control);

    let amp_env = lfo(move |t: f64| {
        let phase = (t * freq_hz as f64).fract();
        0.5 - 0.5 * (2.0 * PI * phase).cos()
    });

    // Apply the modulated delay line
    let shifted = (pass() | mod_sig) >> tap(min_delay, max_delay);
    let shifted_env = shifted * amp_env;

    // Smooth with short decay delay
    let feedback_line = feedback(delay(0.003) * 0.5);

    // Dry/wet mix with feedback on wet path
    let dry = pass() * dc(1.0 - wet_amt);
    let wet = shifted_env >> feedback_line * dc(wet_amt);
    to_net(dry & wet)
}

pub fn master_frequency_shifter(pitch_st: f32, freq_hz: f32, cc: usize, shared_midi_state: &SharedMidiState) -> Net {
    let p_s = pitch_shifter(pitch_st, freq_hz, 1.0);
    cc_controlled_wet_dry_fx(p_s, cc, shared_midi_state)
}
