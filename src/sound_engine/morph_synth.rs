use crate::SharedMidiState;
use crate::common::envelopes::assemble_cc_adsr;
use crate::common::fm::FmConnector;
use crate::common::fundsp::to_net;
use crate::common::helpers::quantize_01_decimal;
use crate::common::modulators::detune_map_semitone;
use crate::common::params::{CcParam, NonCcParam, ParamNode, ParamType, Parameterized};
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
    let (a, d, s, r) = params.get_cc_adsr_params("attack", "decay", "sustain", "release", state);
    let cc_adsr = assemble_cc_adsr(a, d, s, r);

    let osc_a1 = params.get_node_type("osc_a1").unwrap().get_node();
    let osc_a2 = params.get_node_type("osc_a2").unwrap().get_node();
    let osc_b1 = params.get_node_type("osc_b1").unwrap().get_node();
    let osc_b2 = params.get_node_type("osc_b2").unwrap().get_node();

    let detune1 = params.sound_cc_or_default("detune1", state) >> detune_map_semitone();
    let detune2 = params.sound_cc_or_default("detune2", state) >> detune_map_semitone();

    let base_pitch1 = state.bent_pitch() * detune1;
    let base_pitch2 = state.bent_pitch() * detune2;

    // CC: goes from 0.0 to 100 in whole steps
    let fm_ratio_an =
        params.sound_cc_or_default("fm_ratio", state) >> quantize_01_decimal() * constant(100.0);
    let fm_amount_1 = params.sound_cc_or_default("fm_amount_1", state) * constant(13.0);
    let fm_amount_2 = params.sound_cc_or_default("fm_amount_2", state) * constant(13.0);

    let balance1_cc = params.sound_cc_or_default("balance_1", state);
    let balance2_cc = params.sound_cc_or_default("balance_2", state);

    // The B oscillators are modulated by the A oscillators
    let osc_a2 = FmConnector {
        modulator: osc_a1.clone(),
        carrier: osc_a2,
        ratio: to_net(fm_ratio_an.clone()),
        amount: to_net(fm_amount_1),
    }
    .connect_operators(base_pitch1.clone());

    let osc_b2 = FmConnector {
        modulator: osc_b1.clone(),
        carrier: osc_b2,
        ratio: to_net(fm_ratio_an),
        amount: to_net(fm_amount_2),
    }
    .connect_operators(base_pitch2.clone());

    // todo: add env control over moog style filter with same adsr?
    let morph1 = base_pitch1 >> osc_a1 * (constant(1.0) - balance1_cc.clone())
        & osc_a2 * balance1_cc.clone();
    let morph2 =
        base_pitch2 >> osc_b1 * (constant(1.0) - balance2_cc.clone()) & osc_b2 * balance2_cc;
    let synth = Box::new(morph1 + morph2);
    state.assemble_pitched_sound(synth, params.boxed_cc_adsr(cc_adsr, state))
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
                    "The morphing depth of Oscillator1: moves between osc_a1 and osc_a2",
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
            CcParam {
                value: ParamType::ZeroTenFloat(0.005),
                cc_norm_index: 5,
                name: "attack",
                description: Some("attack rate: with CC goes from 0.0 to 5 seconds"),
            },
            CcParam {
                value: ParamType::ZeroTenFloat(0.1),
                cc_norm_index: 6,
                name: "decay",
                description: Some("decay rate: with CC goes from 0.0 to 5 seconds"),
            },
            CcParam {
                value: ParamType::ZeroOneFloat(1.0),
                cc_norm_index: 7,
                name: "sustain",
                description: Some("sustain level from 0.0 to 1.0"),
            },
            CcParam {
                value: ParamType::ZeroTenFloat(0.2),
                cc_norm_index: 8,
                name: "release",
                description: Some("decay rate: with CC goes from 0.0 to 5 seconds"),
            },
        ])),
        non_cc_params: Some(Cow::Borrowed(&[
            NonCcParam {
                value: ParamType::Oscillator(Cow::Borrowed("triangle")),
                name: "osc_a1",
                description: None,
            },
            NonCcParam {
                value: ParamType::Oscillator(Cow::Borrowed("square")),
                name: "osc_a2",
                description: None,
            },
            NonCcParam {
                value: ParamType::Oscillator(Cow::Borrowed("organ")),
                name: "osc_b1",
                description: None,
            },
            NonCcParam {
                value: ParamType::Oscillator(Cow::Borrowed("saw")),
                name: "osc_b2",
                description: None,
            },
        ])),
    },
};
