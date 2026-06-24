use crate::SharedMidiState;
use crate::common::fm::FmConnector;
use crate::common::fundsp::to_net;
use crate::common::helpers::quantize_01_decimal;
use crate::common::params::{CcParam, NonCcParam, ParamNode, ParamType, Parameterized};
use crate::sound_engine::common::detune_map_semitone;
use crate::sound_engine::sound_building::{SOUNDS, SoundFactory};
use fundsp::audiounit::AudioUnit;
use fundsp::prelude64::*;
use linkme::distributed_slice;
use std::borrow::Cow;

/// Morphing Synth engine with FM capabilities.
/// This engine is based on 2 core Oscillators, with each assigned an A oscillator and a B oscillators which can morph into each other.
///
/// User CC control can be assigned to:
/// The balance of A and B (morph depth) per oscillator,
/// Overall detuning of the oscillator (with cc: between -1 and +1 semitones, with config: any f32 value),
/// FM amount: How much A will modulate B (0.0 to 1.0) per oscillator,
/// FM ratio: between 0.0 and 100.0 for both oscillators.
///
/// Configuration can assign:
/// A and B oscillators for each core-Oscillator,
/// ADSR envelope for each synth voice (global).
pub fn morph2(state: &SharedMidiState, params: &Parameterized) -> Box<dyn AudioUnit> {
    let osc1_a = params.get_node_type("osc1_a").unwrap().get_node();
    let osc1_b = params.get_node_type("osc1_b").unwrap().get_node();
    let osc2_a = params.get_node_type("osc2_a").unwrap().get_node();
    let osc2_b = params.get_node_type("osc2_b").unwrap().get_node();

    let detune1 = params.cc_sound_or_default("detune1", state) >> detune_map_semitone();
    let detune2 = params.cc_sound_or_default("detune2", state) >> detune_map_semitone();

    let base_pitch1 = state.bent_pitch() * detune1;
    let base_pitch2 = state.bent_pitch() * detune2;

    // CC: goes from 0.0 to 100 in whole steps
    let fm_ratio_an =
        params.cc_sound_or_default("fm_ratio", state) >> quantize_01_decimal() * constant(100.0);
    let fm_amount_1 = params.cc_sound_or_default("fm_amount_1", state) * constant(13.0);
    let fm_amount_2 = params.cc_sound_or_default("fm_amount_2", state) * constant(13.0);

    let balance1_cc = params.cc_sound_or_default("balance_1", state);
    let balance2_cc = params.cc_sound_or_default("balance_2", state);

    // The B oscillators are modulated by the A oscillators
    let osc1_b = FmConnector {
        modulator: osc1_a.clone(),
        carrier: osc1_b,
        ratio: to_net(fm_ratio_an.clone()),
        amount: to_net(fm_amount_1),
    }
    .connect_operators(base_pitch1.clone());

    let osc2_b = FmConnector {
        modulator: osc2_a.clone(),
        carrier: osc2_b,
        ratio: to_net(fm_ratio_an),
        amount: to_net(fm_amount_2),
    }
    .connect_operators(base_pitch2.clone());

    let morph1 = base_pitch1 >> osc1_a * (constant(1.0) - balance1_cc.clone())
        & osc1_b * balance1_cc.clone();
    let morph2 =
        base_pitch2 >> osc2_a * (constant(1.0) - balance2_cc.clone()) & osc2_b * balance2_cc;
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
                    "The morphing depth of Oscillator1: moves between osc1_a and osc1_b",
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
                value: ParamType::Float32(7.0),
                cc_norm_index: 0,
                name: "fm_ratio",
                description: None,
            },
            CcParam {
                value: ParamType::ZeroOneFloat(0.5),
                cc_norm_index: 0,
                name: "detune1",
                description: None,
            },
            CcParam {
                value: ParamType::ZeroOneFloat(0.5),
                cc_norm_index: 0,
                name: "detune2",
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
