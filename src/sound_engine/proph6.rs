use crate::SharedMidiState;
use crate::common::params::{LFO, ParamNode, Parameterized};
use fundsp::audiounit::AudioUnit;
use std::str::FromStr;

pub fn proph6(state: &SharedMidiState, params: &Parameterized) -> Box<dyn AudioUnit> {
    // osc A
    let osc_a1 = params.get_non_cc_param("osc_a1").unwrap();
    let osc_a2 = params.get_non_cc_param("osc_a2").unwrap();
    let osc_a3 = params.get_non_cc_param("osc_a2").unwrap();
    let osc_a_level = params.sound_cc_or_default("osc_a_level", state);
    let osc_a_pitch_shift = params.sound_cc_or_default("osc_a_pitch_shift", state);
    // let osc_a_master = pitch (is modulated by b?) + pitch_shift semitones -5 -- +5 >> (all osc bus * level)

    // osc B
    let osc_b1 = params.get_non_cc_param("osc_b1").unwrap();
    let osc_b2 = params.get_non_cc_param("osc_b2").unwrap();
    let osc_b3 = params.get_non_cc_param("osc_b2").unwrap();
    let osc_b_level = params.sound_cc_or_default("osc_b_level", state);

    // filter
    // missing cut off, q
    let filter_env_amount = (params.sound_cc_or_default("filter_env_amount", state) * 2.0) - 1.0; // -1 to +1

    // envelops
    let (a, d, s, r) = params.get_cc_adsr_params("attack", "decay", "sustain", "release", state);
    let (mod_a, mod_d, mod_s, mod_r) = params.get_cc_adsr_params(
        "mod_attack",
        "mod_decay",
        "mod_sustain",
        "mod_release",
        state,
    );

    // modulation
    let osc_b_mod_depth = params.sound_cc_or_default("osc_b_mod", state);
    let lfo_freq = params.sound_cc_or_default("lfo_freq", state);
    let lfo_depth = params.sound_cc_or_default("lfo_freq", state);
    let lfo_string = params
        .get_non_cc_param("lfo_shape")
        .expect("did not provide proper lfo shape!")
        .value
        .to_string();
    let lfo_node = lfo_freq * lfo_depth >> LFO::from_str(lfo_string.as_str()).unwrap().get_node();

    todo!("still incomplete")
}
