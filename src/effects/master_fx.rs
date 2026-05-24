use crate::common_definitions::params::ParamInfo;
use crate::common_definitions::params::Parameterized;
use crate::effects::effects_building::EffectDef;
use crate::effects::effects_building::EffectFunc;
use crate::effects::helpers::cc_controlled_wet_dry_fx;
use crate::effects::modulators::{smooth_noise_constructor, smooth_random_lfo};
use crate::effects::params::{Eq2Params, NoParams, ReverbParams};
use crate::helpers::fundsp::to_net;
use crate::{SharedMidiState, register_effect};
use fundsp::prelude64::*;
use std::collections::HashMap;

pub fn master_limiter() -> Net {
    let block = dcblock() >> limiter(0.002, 0.3);
    let master = multipass::<U2>() >> (block.clone() | block);
    to_net(master)
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

pub fn tape_effect_factory(_params: &NoParams, cc_map: &HashMap<String, usize>) -> EffectFunc {
    let depth_val = *cc_map.get("depth").unwrap_or(&0);
    Box::new(move |state| {
        let depth_net = to_net(state.get_fx_cc_or(depth_val, 0.32));
        tape_wow(depth_net)
    })
}

register_effect!(
    name: "tape_drift",
    params: NoParams,
    factory: tape_effect_factory,
    cc_params: [("depth", 2)]
);

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

fn master_lowpass(cc: usize, shared_midi_state: &SharedMidiState, q: f32) -> Net {
    let cutoff_val = shared_midi_state.get_fx_cc_or(cc, 1.0);
    let cutoff_hrz = product(constant(20_000.0), cutoff_val);
    Net::wrap(Box::new(
        (pass() | cutoff_hrz >> follow(0.05_f32)) >> moog_q(q),
    ))
}

fn master_highpass(cc: usize, shared_midi_state: &SharedMidiState, q: f32) -> Net {
    let cutoff_val = shared_midi_state.get_fx_cc_or(cc, 0.0);
    let cutoff_hrz = product(constant(8_000.0), cutoff_val);
    Net::wrap(Box::new(
        (pass() | cutoff_hrz >> follow(0.05_f32)) >> highpass_q(q),
    ))
}

fn eq_2(
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
