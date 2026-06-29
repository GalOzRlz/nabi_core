use crate::SharedMidiState;
use crate::common::adapters::StaticParamsAudioNodeAdapter;
use crate::common::envelopes::assemble_cc_adsr;
use crate::common::params::ParamType::{Float32, Oscillator, String};
use crate::common::params::{CcParam, NonCcParam, ParamType, Parameterized};
use crate::sound_engine::common::{cc_to_cents_by_step, cc_unidirectional_spread_step};
use crate::sound_engine::sound_building::{SOUNDS, SoundFactory};
use fundsp::audiounit::AudioUnit;
use fundsp::net::Net;
use fundsp::prelude::constant;
use fundsp::prelude64::An;
use linkme::distributed_slice;
use std::borrow::Cow;
use std::ops::Add;
use std::sync::Arc;

pub fn super_osc(state: &SharedMidiState, params: &Parameterized) -> Box<dyn AudioUnit> {
    let (a, d, s, r) = params.get_cc_adsr_params("attack", "decay", "sustain", "release", state);
    let cc_adsr = assemble_cc_adsr(a, d, s, r);

    let vc = params.sound_cc_or_default("voice_count", state); // input CC

    let params_owned = params.clone();
    let state_owned = state.clone();

    let mut synth = StaticParamsAudioNodeAdapter::<1, 1>::new(Arc::new(move |args: [f32; 1]| {
        let detune_by = params_owned
            .get_non_cc_param("detune_by")
            .unwrap()
            .value
            .as_string()
            .unwrap();

        let max_spread_hz = params_owned
            .get_non_cc_param("max_spread_hz")
            .unwrap()
            .value
            .as_f32()
            .unwrap();

        let voice_count = if args[0] > 0.3 {
            (args[0] * 20.0) as usize
        } else {
            3.0 as usize
        };
        let pulse_width = params_owned.sound_cc_or_default("pulse_width", &state_owned);
        let detune_spread = params_owned.sound_cc_or_default("detune_spread", &state_owned);
        let osc = params_owned.get_node_type("osc").unwrap().get_pwm_node();
        let mut summing_net = Net::new(0, 1);

        let pitch = state_owned.bent_pitch().clone();

        for num in 0..voice_count {
            let current_pitch = {
                match detune_by {
                    "hz" => {
                        let spread_hz = detune_spread.clone() * max_spread_hz;
                        let spread_step =
                            spread_hz >> cc_unidirectional_spread_step(max_spread_hz, voice_count);
                        let step_val =
                            -constant(max_spread_hz) + (spread_step.clone() * num as f32);
                        pitch.clone().add(step_val)
                    }
                    _ => {
                        (pitch.clone() | detune_spread.clone())
                            >> cc_to_cents_by_step(voice_count, num)
                    }
                }
            };
            let voice = (current_pitch | pulse_width.clone()) >> osc.clone();
            summing_net = summing_net.add(voice);
        }

        println!("Rebuild: voices={}", voice_count);

        summing_net
    }));
    synth.set_fadeout_time(0.1);
    let final_synth = vc >> An(synth);
    let synth = Box::new(final_synth);
    state.assemble_pitched_sound(synth, params.boxed_cc_adsr(cc_adsr, state))
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
            CcParam {
                value: ParamType::Float32(8.0),
                cc_norm_index: 2,
                name: "voice_count",
                description: Some("how many detuned voices per note? from 3 to 20"),
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
                value: Float32(5.5),
                name: "max_spread_hz",
                description: Some(
                    "The maximal frequency for unidirectional spreading (e.g., 20hz means between -20hz and +20hz)",
                ),
            },
            NonCcParam {
                value: String(Cow::Borrowed("cents")),
                name: "detune_by",
                description: Some(
                    "Detuning by \"cents\" or by \"hz\" (maximal range is determined by max_spread_hz)",
                ),
            },
        ])),
    },
};
