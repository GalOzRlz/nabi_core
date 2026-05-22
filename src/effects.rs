use crate::effects_builders::EffectFunc;
use crate::effects_builders::{EffectDef, to_stereo};
use crate::modulators::{smooth_noise_constructor, smooth_random_lfo};
use crate::patch_builder::{ParamDefault, ParamInfo, ParamType, Parameterized};
use crate::{SharedMidiState, register_effect};
use fundsp::combinator::An;
use fundsp::prelude64::*;
use serde::Deserialize;
use std::collections::HashMap;

pub fn to_net<F: AudioNode + 'static>(fx: An<F>) -> Net {
    Net::wrap(Box::new(fx))
}

pub fn master_limiter() -> Net {
    let block = dcblock() >> limiter(0.002, 0.3);
    let master = multipass::<U2>() >> (block.clone() | block);
    to_net(master)
}

pub fn cc_smooth() -> An<Follow<f64>> {
    follow(0.005)
}

fn sensitive_cc_smooth() -> An<Follow<f64>> {
    follow(0.15)
}

/// Factory for stereo effects with wet/dry control via Net  (suitable for live Midi CC)
fn cc_controlled_wet_dry_fx(wet_amount: Net, effect: Net) -> Net {
    // Duplicate wet to stereo (0 inputs, 2 outputs)
    let effect = to_stereo(effect);
    let wet_amount = wet_amount >> cc_smooth();
    let wet_stereo = wet_amount.clone() | wet_amount.clone();

    let dry_mono = constant(1.0) - wet_amount;
    let dry_stereo = dry_mono.clone() | dry_mono;

    let pass = Net::wrap(Box::new(multipass::<U2>())); // U2 -> U2 identity
    (pass * dry_stereo) & (effect * wet_stereo)
}

fn cc_controlled_reverb(wet_amount: Net, reverb_time: f32, room_size: f32, damping: f32) -> Net {
    let reverb = to_net(reverb_stereo(room_size, reverb_time, damping));
    cc_controlled_wet_dry_fx(wet_amount, reverb)
}

pub fn tape_wow(depth: Net) -> Net {
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
    Net::wrap(Box::new(wet_amount.clone() | wet_amount))
}

// pub fn tape_effect_factory(
//     params: &TapeDriftParams,
//     cc_map: &HashMap<String, usize>,
// ) -> EffectFunc {
//     let _ = params.blank;
//     let depth_val = *cc_map.get("depth").unwrap_or(&0);
//     Box::new(move |state| {
//         let depth_net = to_net(state.get_fx_cc(depth_val));
//         tape_wow(depth_net)
//     })
// }

// register_effect!(
//     name: "tape_drift",
//     factory: tape_effect_factory,
//     construction_params: [(blank, 0.0)],
//     cc_params: [("depth", 2, 0.42)]
// );

#[derive(Deserialize)]
#[serde(default)]
pub struct ReverbParams {
    pub room_size: f32,
    pub damping: f32,
    pub length: f32,
}

impl Default for ReverbParams {
    fn default() -> Self {
        Self {
            room_size: 7.8,
            damping: 0.5,
            length: 3.5,
        }
    }
}

impl Parameterized for ReverbParams {
    fn param_info() -> &'static [ParamInfo] {
        &[
            ParamInfo {
                name: "Room Size",
                param_type: ParamType::Float,
                default: ParamDefault::Float(0.8),
                description: Some("The size of the simulated room"),
            },
            ParamInfo {
                name: "Damping",
                param_type: ParamType::Float,
                default: ParamDefault::Float(0.5),
                description: Some(
                    "How much higher frequency suppression will occur in the reverb over time",
                ),
            },
        ]
    }
}

fn fundsp_reverb_factory(params: &ReverbParams, cc_map: &HashMap<String, usize>) -> EffectFunc {
    let room_size = params.room_size; // ← typed, compiler‑checked
    let damping = params.damping;
    let length = params.length;
    let mix_cc = *cc_map.get("wet_amount").unwrap_or(&0);
    Box::new(move |state| {
        let wet_amount = state.get_fx_cc_or(mix_cc, 0.5);
        cc_controlled_reverb(to_net(wet_amount), length, room_size, damping)
    })
}

register_effect!(
    name: "reverb",
    params: ReverbParams,
    factory: fundsp_reverb_factory,
    cc_params: [("wet_amount", 1)]
);

pub fn master_lowpass(cc: usize, shared_midi_state: &SharedMidiState, q: f32) -> Net {
    let cutoff_val = shared_midi_state.get_fx_cc_or(cc, 1.0) >> cc_smooth();
    let cutoff_hrz = product(constant(20_000.0), cutoff_val) >> cc_smooth();
    Net::wrap(Box::new(
        (pass() | cutoff_hrz >> follow(0.05_f32)) >> moog_q(q),
    ))
}

pub fn master_highpass(cc: usize, shared_midi_state: &SharedMidiState, q: f32) -> Net {
    let cutoff_val = shared_midi_state.get_fx_cc_or(cc, 0.0) >> cc_smooth();
    let cutoff_hrz = product(constant(8_000.0), cutoff_val) >> cc_smooth();
    Net::wrap(Box::new(
        (pass() | cutoff_hrz >> follow(0.05_f32)) >> highpass_q(q),
    ))
}

pub fn eq_2(
    low_cut_cc: usize,
    high_cut_cc: usize,
    lp_q: f32,
    hp_q: f32,
    shared_midi_state: &SharedMidiState,
) -> Net {
    let hp = master_highpass(low_cut_cc, shared_midi_state, hp_q.clamp(0.0, 1.3));
    let lp = master_lowpass(high_cut_cc, shared_midi_state, lp_q.clamp(0.0, 1.3));
    multipass::<U2>() >> (lp.clone() | lp) >> (hp.clone() | hp)
}

#[derive(Deserialize)]
#[serde(default)]
pub struct Eq2Params {
    lp_q: f32,
    hp_q: f32,
}

impl Default for Eq2Params {
    fn default() -> Self {
        Self {
            lp_q: 0.1,
            hp_q: 0.1,
        }
    }
}
impl Parameterized for Eq2Params {
    fn param_info() -> &'static [ParamInfo] {
        &[
            ParamInfo {
                name: "Low Pass Q",
                param_type: ParamType::Float,
                default: ParamDefault::Float(0.8),
                description: None,
            },
            ParamInfo {
                name: "High Pass Q",
                param_type: ParamType::Float,
                default: ParamDefault::Float(0.5),
                description: None,
            },
        ]
    }
}
fn eq_2_factory(params: &Eq2Params, cc_map: &HashMap<String, usize>) -> EffectFunc {
    let lp_q = params.lp_q;
    let hp_q = params.hp_q;
    let low_cut = *cc_map.get("lowcut").unwrap_or(&0);
    let high_cut = *cc_map.get("highcut").unwrap_or(&0);
    Box::new(move |state| eq_2(low_cut, high_cut, lp_q, hp_q, state))
}

register_effect!(
    name: "eq2",
    params: Eq2Params,
    factory: eq_2_factory,
    cc_params: [("lowcut", 3), ("highcut", 4)]
);

// todo: add separate high and low as well - filter type selection with enum
