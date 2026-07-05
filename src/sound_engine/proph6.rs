use crate::SharedMidiState;
use crate::common::envelopes::assemble_cc_adsr;
use crate::common::fundsp::to_net;
use crate::common::modulators::to_unipolar;
use crate::common::params::{
    CcParam, LFO, NonCcParam, ParamNode, ParamType, Parameterized, cc_node_to_minus_one,
};
use crate::effects::eqs::prophet_lowpass_filter;
use crate::sound_engine::sound_building::{SOUNDS, SoundFactory};
use fundsp::audiounit::AudioUnit;
use fundsp::math::semitone_ratio;
use fundsp::prelude64::{dc, pass};
use linkme::distributed_slice;
use std::borrow::Cow;
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

    // LFO building
    let lfo_freq = params.sound_cc_or_default("lfo_freq", state) * 100.0;
    let lfo_string = params
        .get_non_cc_param("lfo_shape")
        .expect("did not provide proper lfo shape!")
        .value
        .to_string();
    let lfo_node = lfo_freq >> LFO::from_str(lfo_string.as_str()).unwrap().get_node();

    // LFO Destinations
    let lfo_osc_ab = params.sound_cc_or_default("lfo_pitch_mod_depth", state)
        * lfo_node.clone()
        * semitone_ratio(24.0);
    let lfo_filter =
        params.sound_cc_or_default("lfo_filter_mod_depth", state) * lfo_node.clone() * 5_000.0;
    let lfo_pw = to_unipolar(to_net(
        params.sound_cc_or_default("lfo_pw_mod_depth", state) * lfo_node,
    ));

    // osc level controls?

    // osc B
    let osc_b1 = params.get_node_type("osc_b1").unwrap().get_pwm_node();
    let osc_b2 = params.get_node_type("osc_b2").unwrap().get_pwm_node();
    let osc_b3 = params.get_node_type("osc_b3").unwrap().get_pwm_node();
    let osc_b_level = params.sound_cc_or_default("osc_b_level", state);
    let osc_b_pw = params.sound_cc_or_default("osc_b_pw", state) + lfo_pw.clone();

    let osc_b_pitch_shift = params.cc_to_detune_with_default("osc_b_pitch_shift", state, 5.0);
    let osc_b_master_modulator = ((state.bent_pitch() * osc_b_pitch_shift)
        + (lfo_osc_ab.clone() * state.bent_pitch())
        | osc_b_pw)
        >> (osc_b1 & osc_b2 & osc_b3);
    let osc_b_master = osc_b_master_modulator.clone() * osc_b_level;

    // Poly mod
    let adsr_mod_freq_a =
        cc_node_to_minus_one(params.sound_cc_or_default("adsr_mod_freq_a", state))
            * to_net(mod_adsr.clone())
            * dc(10_000.0);
    let b_mod_a_amplitude =
        (osc_b_master_modulator.clone() * params.sound_cc_or_default("b_mod_a_amp", state));
    let b_mod_filter = params.sound_cc_or_default("b_mod_filter", state);
    let b_mod_filter_cutoff = (osc_b_master_modulator * b_mod_filter) * 5_000.0;

    // osc A
    let osc_a1 = params.get_node_type("osc_a1").unwrap().get_pwm_node();
    let osc_a2 = params.get_node_type("osc_a2").unwrap().get_pwm_node();
    let osc_a3 = params.get_node_type("osc_a3").unwrap().get_pwm_node();
    let osc_a_level = params.sound_cc_or_default("osc_a_level", state);
    let osc_a_pw = params.sound_cc_or_default("osc_a_pw", state) * lfo_pw;

    let osc_a_pitch_shift = params.cc_to_detune_with_default("osc_a_pitch_shift", state, 5.0);
    let osc_a_master = (((state.bent_pitch() * osc_a_pitch_shift)
        + adsr_mod_freq_a
        + (state.bent_pitch() * lfo_osc_ab)
        | osc_a_pw)
        >> (osc_a1 & osc_a2 & osc_a3))
        * osc_a_level
        * to_unipolar(b_mod_a_amplitude);

    // filter
    let filter_cutoff = params.sound_cc_or_default("filter_cutoff", state) * 20_000.0;
    let filter_q = params.sound_cc_or_default("filter_q", state);
    let filter_env_amount =
        cc_node_to_minus_one(params.sound_cc_or_default("filter_env_amount", state));
    let master_filter = (pass()
        | filter_cutoff
            + (b_mod_filter_cutoff * 10_000.0)
            + to_net(mod_adsr) * (filter_env_amount * 10_000.0)
            + lfo_filter
        | filter_q)
        >> prophet_lowpass_filter();

    let noise = params
        .get_noise_node_type("noise_shape")
        .unwrap()
        .get_node()
        * params.sound_cc_or_default("noise_level", state);
    let synth = Box::new((osc_a_master & osc_b_master & noise) >> master_filter);
    state.assemble_pitched_sound(synth, params.boxed_cc_adsr(master_adsr, state))
}
// todo: add level control to each individual osc in A and B - with the global level for each at the end.
#[distributed_slice(SOUNDS)]
static PROPH6: SoundFactory = SoundFactory {
    builder: proph6,
    params: Parameterized {
        name: "proph6",
        cc_params: Some(Cow::Borrowed(&[
            CcParam {
                value: ParamType::ZeroTenFloat(0.5),
                cc_norm_index: 1,
                name: "mod_attack",
                description: Some(
                    "modulator envelopes attack rate: with CC goes from 0.0 to 5 seconds",
                ),
            },
            CcParam {
                value: ParamType::ZeroTenFloat(0.1),
                cc_norm_index: 2,
                name: "mod_decay",
                description: Some(
                    "modulator envelopes decay rate: with CC goes from 0.0 to 5 seconds",
                ),
            },
            CcParam {
                value: ParamType::ZeroOneFloat(0.5),
                cc_norm_index: 3,
                name: "mod_sustain",
                description: Some("modulator envelopes sustain level from 0.0 to 1.0"),
            },
            CcParam {
                value: ParamType::ZeroTenFloat(0.1),
                cc_norm_index: 4,
                name: "mod_release",
                description: Some(
                    "modulator envelopes decay rate: with CC goes from 0.0 to 5 seconds",
                ),
            },
            CcParam {
                value: ParamType::ZeroTenFloat(0.005),
                cc_norm_index: 0,
                name: "attack",
                description: Some("attack rate: with CC goes from 0.0 to 5 seconds"),
            },
            CcParam {
                value: ParamType::ZeroTenFloat(0.1),
                cc_norm_index: 0,
                name: "decay",
                description: Some("decay rate: with CC goes from 0.0 to 5 seconds"),
            },
            CcParam {
                value: ParamType::ZeroOneFloat(1.0),
                cc_norm_index: 0,
                name: "sustain",
                description: Some("sustain level from 0.0 to 1.0"),
            },
            CcParam {
                value: ParamType::ZeroTenFloat(0.2),
                cc_norm_index: 0,
                name: "release",
                description: Some("decay rate: with CC goes from 0.0 to 5 seconds"),
            },
            CcParam {
                value: ParamType::Float32(10.0),
                cc_norm_index: 0,
                name: "lfo_freq",
                description: Some("rate of LFO in hrz"),
            },
            CcParam {
                value: ParamType::ZeroOneFloat(0.0),
                cc_norm_index: 0,
                name: "lfo_pitch_mod_depth",
                description: Some(
                    "0.0 to 1.0 depth for LFO pitch modulation - where 1.0 is two octave",
                ),
            },
            CcParam {
                value: ParamType::ZeroOneFloat(0.0),
                cc_norm_index: 0,
                name: "lfo_filter_mod_depth",
                description: Some("0.0 to 1.0 depth for LFO filter cutoff modulation"),
            },
            CcParam {
                value: ParamType::ZeroOneFloat(0.0),
                cc_norm_index: 0,
                name: "lfo_pw_mod_depth",
                description: Some(
                    "0.0 to 1.0 depth for LFO Pulse width modulation - applies only for the pulse oscillator type",
                ),
            },
            CcParam {
                value: ParamType::ZeroOneFloat(0.9),
                cc_norm_index: 0,
                name: "osc_b_level",
                description: Some("0.0 to 1.0 level for oscillator B"),
            },
            CcParam {
                value: ParamType::ZeroOneFloat(0.8),
                cc_norm_index: 0,
                name: "osc_b_pw",
                description: Some("0.0 to 1.0 pules width value for oscillator B"),
            },
            CcParam {
                value: ParamType::MinusOneToOneFloat(0.0),
                cc_norm_index: 0,
                name: "osc_b_pitch_shift",
                description: Some(
                    "-1.0 to +1.0 detune value for oscillator B - where 1.0 is 5 semitones up and -1 is 5 semitones down.",
                ),
            },
            CcParam {
                value: ParamType::ZeroOneFloat(0.0),
                cc_norm_index: 0,
                name: "noise_level",
                description: Some("0.0 to 1.0 level for noise"),
            },
            CcParam {
                value: ParamType::ZeroOneFloat(0.9),
                cc_norm_index: 0,
                name: "osc_a_level",
                description: Some("0.0 to 1.0 level for oscillator A"),
            },
            CcParam {
                value: ParamType::ZeroOneFloat(0.8),
                cc_norm_index: 0,
                name: "osc_a_pw",
                description: Some("0.0 to 1.0 pules width value for oscillator A"),
            },
            CcParam {
                value: ParamType::MinusOneToOneFloat(-0.05),
                cc_norm_index: 0,
                name: "osc_a_pitch_shift",
                description: Some(
                    "-1.0 to +1.0 detune value for oscillator A - where 1.0 is 5 semitones up and -1 is 5 semitones down.",
                ),
            },
            CcParam {
                value: ParamType::MinusOneToOneFloat(0.0),
                cc_norm_index: 0,
                name: "adsr_mod_freq_a",
                description: Some(
                    "-1.0 to +1.0 detune value for mod ADSR going to oscillator A - where 1.0 is 10,000 khz up and -1 is 10,000 khz down.",
                ),
            },
            CcParam {
                value: ParamType::ZeroOneFloat(0.0),
                cc_norm_index: 0,
                name: "b_mod_filter",
                description: Some("0.0 to 1.0 depth of oscillator B modulating filter cutoff"),
            },
            CcParam {
                value: ParamType::ZeroOneFloat(0.0),
                cc_norm_index: 0,
                name: "b_mod_a_amp",
                description: Some(
                    "0.0 to 1.0 depth of oscillator B modulating the amplitude of oscillator A",
                ),
            },
            CcParam {
                value: ParamType::ZeroOneFloat(0.7),
                cc_norm_index: 0,
                name: "filter_cutoff",
                description: Some("0.0 to 1.0 value for filter cutoff - where 1.0 is no cutoff"),
            },
            CcParam {
                value: ParamType::ZeroOneFloat(0.3),
                cc_norm_index: 0,
                name: "filter_q",
                description: Some(
                    "0.0 to 1.0 value for filter q - where around 0.8 feedback is noticeable",
                ),
            },
            CcParam {
                value: ParamType::MinusOneToOneFloat(-0.2),
                cc_norm_index: 5,
                name: "filter_env_amount",
                description: Some(
                    "-1.0 to 1.0 value for filter envelope amount - where negative moves the cut off lower and positive moves it upwards",
                ),
            },
        ])),
        non_cc_params: Some(Cow::Borrowed(&[
            NonCcParam {
                value: ParamType::String(Cow::Borrowed("sine")),
                name: "lfo_shape",
                description: Some(
                    "LFO shape. Possible options are all oscillator types, all noise types, \
                    'smooth' for smooth noise \
                    and 'sample' for sample and hold",
                ),
            },
            NonCcParam {
                value: ParamType::Oscillator(Cow::Borrowed("brown")),
                name: "noise_shape",
                description: None,
            },
            NonCcParam {
                value: ParamType::Oscillator(Cow::Borrowed("none")),
                name: "osc_b1",
                description: None,
            },
            NonCcParam {
                value: ParamType::Oscillator(Cow::Borrowed("saw")),
                name: "osc_b2",
                description: None,
            },
            NonCcParam {
                value: ParamType::Oscillator(Cow::Borrowed("pulse")),
                name: "osc_b3",
                description: None,
            },
            NonCcParam {
                value: ParamType::Oscillator(Cow::Borrowed("none")),
                name: "osc_a1",
                description: None,
            },
            NonCcParam {
                value: ParamType::Oscillator(Cow::Borrowed("saw")),
                name: "osc_a2",
                description: None,
            },
            NonCcParam {
                value: ParamType::Oscillator(Cow::Borrowed("triangle")),
                name: "osc_a3",
                description: None,
            },
        ])),
    },
};
