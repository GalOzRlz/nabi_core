use crate::SharedMidiState;
use crate::common::envelopes::cc_controlled_attack_decay;
use crate::common::params::{CcParam, NonCcParam, ParamNode, ParamType, Parameterized};
use crate::sound_engine::instruments::{Polarity, pluck_comb_string};
use crate::sound_engine::sound_building::{SOUNDS, SoundFactory};
use fundsp::audiounit::AudioUnit;
use linkme::distributed_slice;
use std::borrow::Cow;

pub fn karplus_strong_comb(state: &SharedMidiState, params: &Parameterized) -> Box<dyn AudioUnit> {
    let damping = params.sound_cc_or_default("damping", state);
    let attack = params.sound_cc_or_default("attack", state) * 1.0 + 0.005;
    let decay = params.sound_cc_or_default("decay", state) * 1.0 + 0.005;
    let polarity_param = params
        .get_non_cc_param("polarity")
        .unwrap()
        .value
        .as_string()
        .unwrap();

    let polarity = Polarity::from_string(polarity_param);
    let ks = pluck_comb_string(polarity);

    let excitation_noise = params.get_noise_node_type("excitation").unwrap().get_node();
    let excitation_env = (state.gate_var() | attack | decay) >> cc_controlled_attack_decay();
    let synth = Box::new(
        (state.bent_pitch() | state.gate_var() | excitation_noise * excitation_env | damping)
            >> ks * 1.4,
    );

    state.assemble_pitched_sound(synth, params.boxed_static_adsr("adsr", state))
}

#[distributed_slice(SOUNDS)]
static KS_COMB: SoundFactory = SoundFactory {
    builder: karplus_strong_comb,
    params: Parameterized {
        name: "karplus_strong_comb",
        cc_params: Some(Cow::Borrowed(&[
            CcParam {
                value: ParamType::ZeroOneFloat(0.5),
                cc_norm_index: 1,
                name: "damping",
                description: Some(
                    "damping factor of the physical string - the higher it gets the more higher frequencies are supressed over time and shorter the decay becomes",
                ),
            },
            CcParam {
                value: ParamType::ZeroOneFloat(0.01),
                cc_norm_index: 2,
                name: "attack",
                description: Some(
                    "attack rate for the initial noise excitation - longer attacks create a breathier attack",
                ),
            },
            CcParam {
                value: ParamType::ZeroOneFloat(0.01),
                cc_norm_index: 3,
                name: "decay",
                description: Some(
                    "decay rate for the initial noise excitation - the longer it gets the more ",
                ),
            },
        ])),
        non_cc_params: Some(Cow::Borrowed(&[
            NonCcParam {
                value: ParamType::Oscillator(Cow::Borrowed("white")),
                name: "excitation",
                description: Some(
                    "noise type to use for excitation of the high feedback comb filter\n\
                available variants are \"white\", \"brown\" and \"pink\"",
                ),
            },
            NonCcParam {
                value: ParamType::Oscillator(Cow::Borrowed("positive")),
                name: "polarity",
                description: Some(
                    "filter polarity, can be either positive or negative. Negative value creates a plastic-pipe type of sound",
                ),
            },
        ])),
    },
};
