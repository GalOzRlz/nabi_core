use crate::SharedMidiState;
use crate::common::params::ParamType::{ADSR, Oscillator, U8, ZeroHundredFloat};
use crate::common::params::{CcParam, NonCcParam, ParamType, Parameterized};
use crate::helpers::fundsp::to_net;
use crate::sound_engine::common::cc_unidirectional_spread_step;
use crate::sound_engine::sound_building::{SOUNDS, SoundFactory};
use fundsp::audiounit::AudioUnit;
use fundsp::net::Net;
use fundsp::prelude::constant;
use fundsp::prelude64::panner;
use linkme::distributed_slice;
use std::borrow::Cow;

pub fn super_osc(state: &SharedMidiState, params: &Parameterized) -> Box<dyn AudioUnit> {
    let max_spread_hz = params
        .get_non_cc_param("max_spread_hz")
        .unwrap()
        .value
        .as_f32()
        .unwrap();
    let pulse_width = params.cc_sound_or_default("pulse_width", state);
    let spread_hz = params.cc_sound_or_default("detune_spread", state) * max_spread_hz;
    let osc = params.get_osc_node_type("osc").unwrap().get_osc_node_pw();

    let voice_count = {
        match params.get_non_cc_param("voice_count").unwrap().value {
            U8(x) => x,
            _ => panic!("osc_count value must be U8"),
        }
    };

    let mut summing_net = Net::new(0, 2);
    let spread_step =
        spread_hz.clone() >> cc_unidirectional_spread_step(max_spread_hz, voice_count);
    let volume_factor = 0.9 / voice_count as f32;

    for num in 0..voice_count {
        let step_val = -constant(max_spread_hz) + (spread_step.clone() * num as f32);
        let new_voice = Net::pipe(
            (state.bent_pitch() + step_val.clone() | pulse_width.clone()),
            to_net(osc.clone()),
        );
        summing_net = Net::sum(
            summing_net,
            (new_voice | step_val * (1.0 / max_spread_hz)) >> panner(),
        ) * volume_factor;
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
                description: Some("The amount of spread detuning"),
            },
        ])),
        non_cc_params: Some(Cow::Borrowed(&[
            NonCcParam {
                value: Oscillator(Cow::Borrowed("triangle")),
                name: "osc",
                description: None,
            },
            NonCcParam {
                value: ADSR([0.02, 0.9, 0.75, 0.35]),
                name: "adsr",
                description: None,
            },
            NonCcParam {
                value: U8(10),
                name: "voice_count",
                description: None,
            },
            NonCcParam {
                value: ZeroHundredFloat(50.0),
                name: "max_spread_hz",
                description: None,
            },
        ])),
    },
};
