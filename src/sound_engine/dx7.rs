use crate::SharedMidiState;
use crate::common::adapters::StaticParamsAudioNodeAdapter;
use crate::common::envelopes::assemble_cc_adsr;
use crate::common::fundsp::to_net;
use crate::common::params::ParamType::{Float32, String};
use crate::common::params::{CcParam, NonCcParam, ParamType, Parameterized};
use crate::sound_engine::sound_building::{SOUNDS, SoundFactory};
use cpal::SAMPLE_RATE_CD;
use fundsp::audiounit::AudioUnit;
use fundsp::prelude64::An;
use fundx7::fm::voice::{Parameters, Voice};
use fundx7::*;
use linkme::distributed_slice;
use std::borrow::Cow;
use std::sync::Arc;

pub fn dx7_sysex(state: &SharedMidiState, params: &Parameterized) -> Box<dyn AudioUnit> {
    let (a, d, s, r) = params.get_cc_adsr_params("attack", "decay", "sustain", "release", state);
    let cc_adsr = assemble_cc_adsr(a, d, s, r);

    let patch_path = params
        .get_non_cc_param("file_path")
        .expect("No sysex file path specified")
        .value
        .as_string()
        .unwrap();
    let patch_bank = std::fs::read(patch_path).unwrap();

    let patch_bank = PatchBank::new(&patch_bank);
    let patch = patch_bank.patches[params
        .get_non_cc_param("patch_num")
        .unwrap()
        .value
        .as_f32()
        .unwrap() as usize];

    let fm_synth = Voice::new(patch, Parameters::default(), SAMPLE_RATE_CD as f32);
    let synth = (state.note_var() | state.gate_var() | state.velocity_var()) >> An(fm_synth) * 0.5;
    state.assemble_pitched_sound(Box::new(synth), params.boxed_cc_adsr(cc_adsr, state))
}

#[distributed_slice(SOUNDS)]
static DX7_SYSEX: SoundFactory = SoundFactory {
    builder: dx7_sysex,
    params: Parameterized {
        name: "dx7_sysex",
        cc_params: Some(Cow::Borrowed(&[
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
                value: ParamType::ZeroTenFloat(0.5),
                cc_norm_index: 8,
                name: "release",
                description: Some("decay rate: with CC goes from 0.0 to 5 seconds"),
            },
        ])),
        non_cc_params: Some(Cow::Borrowed(&[
            NonCcParam {
                value: Float32(7.0),
                name: "patch_num",
                description: Some(
                    "The patch number of the sound stored in the sysex file. While the library will start at 1 our indexing will start at 0.",
                ),
            },
            NonCcParam {
                value: String(Cow::Borrowed("sysex/star1-fast-decay.syx")),
                name: "file_path",
                description: Some(
                    "The patch number of the sound stored in the sysex file. While the library will start at 1 our indexing will start at 0.",
                ),
            },
        ])),
    },
};

pub fn dx7_scroller(state: &SharedMidiState, params: &Parameterized) -> Box<dyn AudioUnit> {
    let (a, d, s, r) = params.get_cc_adsr_params("attack", "decay", "sustain", "release", state);
    let cc_adsr = assemble_cc_adsr(a, d, s, r);

    let patch_bank = std::fs::read("sysex/star1-fast-decay.syx").unwrap();

    let patch_bank = PatchBank::new(&patch_bank);
    let mut fm_synth = An(StaticParamsAudioNodeAdapter::<4, 1>::new(Arc::new(
        move |args: [f32; 4]| {
            let step = 1.0 / patch_bank.patches.len() as f32;
            let selected = patch_bank.patches[(args[3] / step).round() as usize];
            to_net(An(Voice::new(
                selected,
                Parameters::default(),
                SAMPLE_RATE_CD as f32,
            )))
        },
    )));
    fm_synth.disable_fadeout();
    fm_synth.rebuild_on_change(|x, y| x[3] != y[3]);
    let cc_scroller = params.sound_cc_or_default("scroll", state);
    let synth = (state.note_var() | state.gate_var() | state.velocity_var() | cc_scroller)
        >> fm_synth * 0.5;
    state.assemble_pitched_sound(Box::new(synth), params.boxed_cc_adsr(cc_adsr, state))
}

#[distributed_slice(SOUNDS)]
static DX7_SCROLL: SoundFactory = SoundFactory {
    builder: dx7_scroller,
    params: Parameterized {
        name: "dx7_scroller",
        cc_params: Some(Cow::Borrowed(&[
            CcParam {
                value: ParamType::ZeroOneFloat(0.0),
                cc_norm_index: 1,
                name: "scroll",
                description: Some(
                    "scrolling cc that will change patches within the sysex file on the fly",
                ),
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
                value: ParamType::ZeroTenFloat(0.5),
                cc_norm_index: 8,
                name: "release",
                description: Some("decay rate: with CC goes from 0.0 to 5 seconds"),
            },
        ])),
        non_cc_params: Some(Cow::Borrowed(&[NonCcParam {
            value: String(Cow::Borrowed("sysex/star1-fast-decay.syx")),
            name: "file_path",
            description: Some(
                "The patch number of the sound stored in the sysex file. While the library will start at 1 our indexing will start at 0.",
            ),
        }])),
    },
};
