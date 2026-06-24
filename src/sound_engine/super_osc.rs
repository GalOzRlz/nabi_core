use crate::SharedMidiState;
use crate::common::fundsp::to_net;
use crate::common::params::ParamType::{ADSR, Float32, Oscillator, U8};
use crate::common::params::{CcParam, NonCcParam, ParamType, Parameterized};
use crate::sound_engine::common::cc_unidirectional_spread_step;
use crate::sound_engine::sound_building::{SOUNDS, SoundFactory};
use fundsp::audiounit::AudioUnit;
use fundsp::net::Net;
use fundsp::prelude::constant;
use linkme::distributed_slice;
use std::borrow::Cow;
use std::ops::Add;

/// add live adsr
pub fn super_osc(state: &SharedMidiState, params: &Parameterized) -> Box<dyn AudioUnit> {
    let max_spread_hz = params
        .get_non_cc_param("max_spread_hz")
        .unwrap()
        .value
        .as_f32()
        .unwrap();
    let pulse_width = params.cc_sound_or_default("pulse_width", state);
    let spread_hz = params.cc_sound_or_default("detune_spread", state) * max_spread_hz;
    let osc = params.get_node_type("osc").unwrap().get_node_pw();

    let voice_count = {
        match params.get_non_cc_param("voice_count").unwrap().value {
            U8(x) => x,
            _ => panic!("osc_count value must be U8"),
        }
    };

    let mut summing_net = Net::new(0, 1);
    let spread_step =
        spread_hz.clone() >> cc_unidirectional_spread_step(max_spread_hz, voice_count);

    for num in 0..voice_count {
        let step_val = -constant(max_spread_hz) + (spread_step.clone() * num as f32);
        let new_voice = Net::pipe(
            (state.bent_pitch().add(step_val.clone()) | pulse_width.clone()),
            to_net(osc.clone()),
        );
        summing_net = summing_net.add(new_voice);
    }
    let synth = Box::new(summing_net);
    state.assemble_pitched_sound(synth, params.boxed_adsr("adsr", state))
}

#[distributed_slice(SOUNDS)]
static SUPER_OSC: SoundFactory = SoundFactory {
    builder: super_osc,
    params: Parameterized {
        name: "super_osc",
        cc_params: Some(Cow::Borrowed(&[
            CcParam {
                value: ParamType::ZeroOneFloat(0.5),
                cc_norm_index: 0,
                name: "pulse_width",
                description: Some("Pulse width amount for the Pulse oscillator"),
            },
            CcParam {
                value: ParamType::ZeroOneFloat(0.15),
                cc_norm_index: 1,
                name: "detune_spread",
                description: Some("The amount of spread for voice detuning"),
            },
        ])),
        non_cc_params: Some(Cow::Borrowed(&[
            NonCcParam {
                value: Oscillator(Cow::Borrowed("saw")),
                name: "osc",
                description: None,
            },
            NonCcParam {
                value: ADSR([0.02, 0.9, 0.75, 0.35]),
                name: "adsr",
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
