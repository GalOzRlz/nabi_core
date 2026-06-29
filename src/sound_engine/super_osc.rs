use crate::SharedMidiState;
use crate::common::envelopes::assemble_cc_adsr;
use crate::common::params::ParamType::{Float32, Oscillator, U8};
use crate::common::params::{CcParam, NonCcParam, ParamNode, ParamType, Parameterized};
use crate::sound_engine::instruments::SuperOSC;
use crate::sound_engine::sound_building::{SOUNDS, SoundFactory};
use fundsp::audiounit::AudioUnit;
use fundsp::prelude64::An;
use linkme::distributed_slice;
use std::borrow::Cow;

pub fn super_osc(state: &SharedMidiState, params: &Parameterized) -> Box<dyn AudioUnit> {
    let (a, d, s, r) = params.get_cc_adsr_params("attack", "decay", "sustain", "release", state);
    let cc_adsr = assemble_cc_adsr(a, d, s, r);

    let max_spread_hz = params
        .get_non_cc_param("max_spread_hz")
        .unwrap()
        .value
        .as_f32()
        .unwrap();

    let detune_spread = params.sound_cc_or_default("detune_spread", state);
    let voice_count = params.sound_cc_or_default("voice_count", state) * 100.0;

    let osc = params.get_node_type("osc").unwrap().as_audiounit();
    let synth = Box::new(
        (state.bent_pitch() | voice_count)
            >> An(SuperOSC::new(osc, detune_spread, 100, max_spread_hz)),
    );
    state.assemble_pitched_sound(synth, params.boxed_cc_adsr(cc_adsr, state))
}

#[distributed_slice(SOUNDS)]
static SUPER_OSC: SoundFactory = SoundFactory {
    builder: super_osc,
    params: Parameterized {
        name: "super_osc",
        cc_params: Some(Cow::Borrowed(&[
            CcParam {
                value: ParamType::ZeroOneFloat(0.15),
                cc_norm_index: 1,
                name: "detune_spread",
                description: Some("The amount of spread for voice detuning"),
            },
            CcParam {
                value: ParamType::Float32(8.0),
                cc_norm_index: 2,
                name: "voice_count",
                description: Some("how many detuned voices per note? from 3 to 100"),
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
                value: Oscillator(Cow::Borrowed("saw")),
                name: "osc",
                description: None,
            },
            NonCcParam {
                value: U8(8),
                name: "voice_count",
                description: None,
            },
            NonCcParam {
                value: Float32(5.5),
                name: "max_spread_hz",
                description: Some(
                    "The maximal frequency for unidirectional spreading (e.g., 20hz means between -20hz and +20hz)",
                ),
            },
        ])),
    },
};
