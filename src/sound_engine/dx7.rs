use crate::SharedMidiState;
use crate::common::params::ParamType::Float32;
use crate::common::params::{CcParam, NonCcParam, ParamType, Parameterized};
use crate::sound_engine::sound_building::{SOUNDS, SoundFactory};
use cpal::SAMPLE_RATE_CD;
use fundsp::audiounit::AudioUnit;
use fundsp::prelude64::An;
use fundx7::fm::voice::{Parameters, Voice};
use fundx7::*;
use linkme::distributed_slice;
use std::borrow::Cow;

pub fn dx7_sysex(state: &SharedMidiState, params: &Parameterized) -> Box<dyn AudioUnit> {
    let patch_bank = std::fs::read("sysex/star1-fast-decay.syx").unwrap();

    let patch_bank = PatchBank::new(&patch_bank);
    let my_favorite_patch = patch_bank.patches[params
        .get_non_cc_param("patch_num")
        .unwrap()
        .value
        .as_f32()
        .unwrap() as usize];
    let parameters = Parameters {
        gate: true,
        sustain: false,
        velocity: 1.0,
        note: state.midi_note.value(),
        ..Parameters::default()
    };
    let fm_synth = Voice::new(my_favorite_patch, parameters, SAMPLE_RATE_CD as f32);
    let synth = (state.note_var() | state.gate_var()) >> An(fm_synth);
    Box::new(synth)
}

#[distributed_slice(SOUNDS)]
static SUPER_OSC: SoundFactory = SoundFactory {
    builder: dx7_sysex,
    params: Parameterized {
        name: "dx7_sysex",
        cc_params: Some(Cow::Borrowed(&[
            CcParam {
                value: ParamType::ZeroTenFloat(0.001),
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
                value: ParamType::ZeroTenFloat(0.5),
                cc_norm_index: 8,
                name: "release",
                description: Some("decay rate: with CC goes from 0.0 to 5 seconds"),
            },
        ])),
        non_cc_params: Some(Cow::Borrowed(&[NonCcParam {
            value: Float32(0.0),
            name: "patch_num",
            description: None,
        }])),
    },
};
