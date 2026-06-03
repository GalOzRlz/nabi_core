use crate::SharedMidiState;
use crate::common::fm::FmOperator;
use crate::common::helpers::quantize_01_decimal;
use crate::common::params::{CcParam, NonCcParam, ParamType, Parameterized};
use crate::helpers::fundsp::to_net;
use crate::sound_engine::sound_building::{SOUNDS, SoundFactory};
use fundsp::audiounit::AudioUnit;
use fundsp::prelude64::*;
use linkme::distributed_slice;
use std::borrow::Cow;

// todo: add cc frequency detune control for each operator (+12/-12)
pub fn morph2(state: &SharedMidiState, params: &Parameterized) -> Box<dyn AudioUnit> {
    let osc_1a = params.get_osc_node_type("osc1_a").unwrap().get_osc_node();
    let osc_1b = params.get_osc_node_type("osc1_b").unwrap().get_osc_node();
    let osc_2a = params.get_osc_node_type("osc2_a").unwrap().get_osc_node();
    let osc_2b = params.get_osc_node_type("osc2_b").unwrap().get_osc_node();

    // CC: goes from 0.0 to 100 in whole steps
    let fm_ratio_an =
        params.cc_sound_or_default("fm_ratio", state) >> quantize_01_decimal() * constant(100.0);
    let fm_amount_1 = params.cc_sound_or_default("fm_amount_1", state) * constant(13.0);
    let fm_amount_2 = params.cc_sound_or_default("fm_amount_2", state) * constant(13.0);

    let b1_cc = params.cc_sound_or_default("balance_1", state);
    let b2_cc = params.cc_sound_or_default("balance_2", state);

    // The B oscillators are modulated by the A oscillators
    let osc_1b = FmOperator {
        modulator: osc_1a.clone(),
        carrier: osc_1b,
        ratio: to_net(fm_ratio_an.clone()),
        amount: to_net(fm_amount_1),
    }
    .build_operator(state);

    let osc_2b = FmOperator {
        modulator: osc_2a.clone(),
        carrier: osc_2b,
        ratio: to_net(fm_ratio_an),
        amount: to_net(fm_amount_2),
    }
    .build_operator(state);

    let morph1 =
        (state.bent_pitch() >> osc_1a * (constant(1.0) - b1_cc.clone()) & osc_1b * b1_cc.clone());
    let morph2 = (state.bent_pitch() >> osc_2a * (constant(1.0) - b2_cc.clone()) & osc_2b * b2_cc);
    let synth = Box::new(morph1 + morph2);
    state.assemble_pitched_sound(synth, params.boxed_adsr("adsr", state))
}

#[distributed_slice(SOUNDS)]
static MORPH2: SoundFactory = SoundFactory {
    builder: morph2,
    params: Parameterized {
        name: "morph2",
        cc_params: Some(Cow::Borrowed(&[
            CcParam {
                value: ParamType::ZeroOneFloat(0.4),
                cc_norm_index: 1,
                name: "balance_1",
                description: Some(
                    "The morphing depth of Oscillator1: moves between osc_1a and osc_1b",
                ),
            },
            CcParam {
                value: ParamType::ZeroOneFloat(0.5),
                cc_norm_index: 2,
                name: "balance_2",
                description: None,
            },
            CcParam {
                value: ParamType::ZeroOneFloat(0.0),
                cc_norm_index: 3,
                name: "fm_amount_1",
                description: None,
            },
            CcParam {
                value: ParamType::ZeroOneFloat(0.0),
                cc_norm_index: 4,
                name: "fm_amount_2",
                description: None,
            },
            CcParam {
                value: ParamType::ZeroHundredFloat(7.0),
                cc_norm_index: 0, // static value by default
                name: "fm_ratio",
                description: None,
            },
        ])),
        non_cc_params: Some(Cow::Borrowed(&[
            NonCcParam {
                value: ParamType::Oscillator(Cow::Borrowed("triangle")),
                name: "osc1_a",
                description: None,
            },
            NonCcParam {
                value: ParamType::Oscillator(Cow::Borrowed("square")),
                name: "osc1_b",
                description: None,
            },
            NonCcParam {
                value: ParamType::Oscillator(Cow::Borrowed("sine")),
                name: "osc2_a",
                description: None,
            },
            NonCcParam {
                value: ParamType::Oscillator(Cow::Borrowed("saw")),
                name: "osc2_b",
                description: None,
            },
            NonCcParam {
                value: ParamType::ADSR([0.3, 0.1, 0.75, 0.35]),
                name: "adsr",
                description: None,
            },
        ])),
    },
};

//todo: add a general synth: pro6 style...2 oscillators with shapes cascading (saw, triangle, pulse) - detune control,
