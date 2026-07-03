use crate::SharedMidiState;
use crate::common::envelopes::assemble_cc_adsr;
use crate::common::fundsp::to_net;
use crate::common::params::{LFO, ParamNode, Parameterized};
use crate::effects::eqs::prophet_lowpass_filter;
use fundsp::audiounit::AudioUnit;
use fundsp::math::semitone_ratio;
use fundsp::prelude64::{dc, pass};
use std::str::FromStr;

pub fn proph6(state: &SharedMidiState, params: &Parameterized) -> Box<dyn AudioUnit> {
    // envelops
    let (a, d, s, r) = params.get_cc_adsr_params("attack", "decay", "sustain", "release", state);
    let (mod_a, mod_d, mod_s, mod_r) = params.get_cc_adsr_params(
        "mod_attack",
        "mod_decay",
        "mod_sustain",
        "mod_release",
        state,
    );
    let master_adsr = assemble_cc_adsr(a, d, s, r);
    let mod_adsr = state.gate_var() >> assemble_cc_adsr(mod_a, mod_d, mod_s, mod_r);

    // LFO
    let lfo_freq = params.sound_cc_or_default("lfo_freq", state);
    let lfo_depth = params.sound_cc_or_default("lfo_freq", state);
    let lfo_string = params
        .get_non_cc_param("lfo_shape")
        .expect("did not provide proper lfo shape!")
        .value
        .to_string();
    let lfo_node = lfo_freq * lfo_depth >> LFO::from_str(lfo_string.as_str()).unwrap().get_node();

    let lfo_osc_ab = params.sound_cc_or_default("lfo_osc_mod_depth", state) * lfo_node.clone();
    let lfo_filter = params.sound_cc_or_default("lfo_filter_mod_depth", state) * lfo_node.clone();
    let lfo_pw = params.sound_cc_or_default("lfo_pw_mod_depth", state) * lfo_node;
    let lfo_osc_pitch_ab = lfo_osc_ab * semitone_ratio(5.0);

    // osc B
    let osc_b1 = params.get_node_type("osc_b1").unwrap().get_pwm_node();
    let osc_b2 = params.get_node_type("osc_b2").unwrap().get_pwm_node();
    let osc_b3 = params.get_node_type("osc_b3").unwrap().get_pwm_node();
    let osc_b_level = params.sound_cc_or_default("osc_b_level", state);
    let osc_b_pw = params.sound_cc_or_default("osc_b_pw", state) * lfo_pw.clone();

    let osc_b_pitch_shift = params.cc_to_detune_with_default("osc_b_pitch_shift", state, 5.0);
    let osc_b_master_modulator =
        ((state.bent_pitch() * osc_b_pitch_shift) + lfo_osc_pitch_ab.clone() | osc_b_pw)
            >> (osc_b1 & osc_b2 & osc_b3);
    let osc_b_master = osc_b_master_modulator.clone() * osc_b_level + lfo_osc_pitch_ab.clone();

    // Poly mod
    let adsr_mod_freq_a =
        params.cc_to_detune_with_default("adsr_mod_freq_a", state, 5.0) * mod_adsr.clone();
    let osc_b_to_a_am = params.sound_cc_or_default("osc_b_to_a_am", state);
    let osc_b_to_filter_amount = params.sound_cc_or_default("osc_b_to_filter_amount", state);
    let b_mod_a_amplitude = (osc_b_master_modulator.clone() * osc_b_to_a_am);

    // osc A
    let osc_a1 = params.get_node_type("osc_a1").unwrap().get_node();
    let osc_a2 = params.get_node_type("osc_a2").unwrap().get_node();
    let osc_a3 = params.get_node_type("osc_a3").unwrap().get_node();
    let osc_a_level = params.sound_cc_or_default("osc_a_level", state);
    let osc_a_pw = params.sound_cc_or_default("osc_a_pw", state) * lfo_pw;

    let osc_a_pitch_shift = params.cc_to_detune_with_default("osc_a_pitch_shift", state, 5.0);
    let osc_a_master = ((state.bent_pitch() * osc_a_pitch_shift)
        + (to_net(adsr_mod_freq_a) * dc(semitone_ratio(5.0)))
        + lfo_osc_pitch_ab
        | osc_a_pw)
        >> (osc_a1 & osc_a2 & osc_a3) * osc_a_level * b_mod_a_amplitude;

    // filter
    let filter_cutoff = params.sound_cc_or_default("filter_cutoff", state) * 20_000.0;
    let filter_q = params.sound_cc_or_default("filter_q", state);
    let b_mod_filter_cutoff = osc_b_master_modulator * osc_b_to_filter_amount;
    let filter_env_amount = params.sound_cc_or_default("filter_env_amount", state);
    let master_filter = (pass()
        | filter_cutoff
            + (b_mod_filter_cutoff * 5_000.0)
            + (to_net(mod_adsr) * filter_env_amount * 5_000.0)
            + (lfo_filter * 5_000.0)
        | filter_q)
        >> prophet_lowpass_filter();

    let synth = Box::new((osc_a_master + osc_b_master) >> master_filter);
    state.assemble_pitched_sound(synth, params.boxed_cc_adsr(master_adsr, state))
    // todo: make the osc_b > pitch_a into AM modulation instead! we got fm already in morph2 so it can be more interesting to have am...
}
