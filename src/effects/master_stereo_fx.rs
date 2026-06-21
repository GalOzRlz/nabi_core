use crate::common::params::{CcAudioNode, CcParam, NonCcParam, ParamType, Parameterized};
use crate::effects::effects_building::EffectFunc;
use crate::effects::effects_building::{EFFECTS, EffectDef};
use crate::effects::helpers::cc_controlled_wet_dry_fx;
use crate::effects::pitch_modulation::{pitch_shifter, tape_wow};
use crate::helpers::fundsp::to_net;
use fundsp::prelude64::*;
use linkme::distributed_slice;
use std::borrow::Cow;
use std::sync::Arc;

pub fn master_limiter() -> Net {
    let block = dcblock() >> limiter(0.002, 0.3);
    let master = multipass::<U2>() >> (block.clone() | block);
    to_net(master)
}

fn cc_controlled_reverb(
    wet_amount: CcAudioNode,
    reverb_time: f32,
    room_size: f32,
    damping: f32,
) -> Net {
    let reverb = to_net(reverb_stereo(room_size, reverb_time, damping));
    cc_controlled_wet_dry_fx(wet_amount, reverb)
}

fn fundsp_reverb_factory(params: Arc<Parameterized>) -> EffectFunc {
    Box::new(move |state| {
        let room_size_param = params.get_non_cc_param("room_size").unwrap();
        let damping_param = params.get_non_cc_param("damping").unwrap();
        let length_param = params.get_non_cc_param("length").unwrap();
        let wet_amount = params.cc_fx_or_default("wet_amount", state);
        cc_controlled_reverb(
            wet_amount,
            length_param.value.as_f32().unwrap(),
            room_size_param.value.as_f32().unwrap(),
            damping_param.value.as_zero_to_one_f32().unwrap(),
        )
    })
}

#[distributed_slice(EFFECTS)]
static REVERB: EffectDef = EffectDef {
    factory: fundsp_reverb_factory,
    params: Parameterized {
        name: "reverb",
        cc_params: Some(Cow::Borrowed(&[CcParam {
            value: ParamType::ZeroOneFloat(0.35),
            cc_norm_index: 1,
            name: "wet_amount",
            description: None,
        }])),
        non_cc_params: Some(Cow::Borrowed(&[
            NonCcParam {
                value: ParamType::Float32(8.0),
                name: "room_size",
                description: None,
            },
            NonCcParam {
                value: ParamType::ZeroOneFloat(0.55),
                name: "damping",
                description: None,
            },
            NonCcParam {
                value: ParamType::Float32(4.35),
                name: "length",
                description: None,
            },
        ])),
    },
};

fn eq2(params: Arc<Parameterized>) -> EffectFunc {
    Box::new(move |state| {
        let lowpass_cutoff = params.cc_fx_or_default("lowpass_cutoff", state);
        let lowpass_q = params.cc_fx_or_default("lowpass_q", state);
        let highpass_cutoff = params.cc_fx_or_default("highpass_cutoff", state);
        let highpass_q = params.cc_fx_or_default("highpass_q", state);

        let lp_max_frequency = params
            .get_non_cc_param("lp_max_frequency")
            .unwrap()
            .value
            .as_f32()
            .unwrap();
        let hp_max_frequency = params
            .get_non_cc_param("hp_max_frequency")
            .unwrap()
            .value
            .as_f32()
            .unwrap();

        let lowpass_cutoff = product(constant(lp_max_frequency), lowpass_cutoff);
        let highpass_cutoff = product(constant(hp_max_frequency), highpass_cutoff);

        let lp = (pass() | lowpass_cutoff | lowpass_q) >> moog();
        let hp = (pass() | highpass_cutoff | highpass_q) >> highpass();

        to_net(multipass::<U2>() >> (lp.clone() | lp) >> (hp.clone() | hp))
    })
}

#[distributed_slice(EFFECTS)]
static EQ2: EffectDef = EffectDef {
    factory: eq2,
    params: Parameterized {
        name: "eq2",
        cc_params: Some(Cow::Borrowed(&[
            CcParam {
                value: ParamType::ZeroOneFloat(1.0),
                cc_norm_index: 4,
                name: "lowpass_cutoff",
                description: None,
            },
            CcParam {
                value: ParamType::ZeroOneFloat(0.6),
                cc_norm_index: 0,
                name: "lowpass_q",
                description: None,
            },
            CcParam {
                value: ParamType::ZeroOneFloat(0.0),
                cc_norm_index: 3,
                name: "highpass_cutoff",
                description: None,
            },
            CcParam {
                value: ParamType::ZeroOneFloat(0.35),
                cc_norm_index: 0,
                name: "lowpass_q",
                description: None,
            },
        ])),
        non_cc_params: Some(Cow::Borrowed(&[
            NonCcParam {
                value: ParamType::Float32(15_000.0),
                name: "lp_max_frequency",
                description: Some("The top frequency the high-cut will go to"),
            },
            NonCcParam {
                value: ParamType::Float32(8_000.0),
                name: "hp_max_frequency",
                description: Some("The top frequency the low-cut will go to"),
            },
        ])),
    },
};

pub fn tape_drift(params: Arc<Parameterized>) -> EffectFunc {
    Box::new(move |state| {
        let depth_net = params.cc_fx_or_default("drift_depth", state);
        tape_wow(depth_net)
    })
}

#[distributed_slice(EFFECTS)]
static TAPE_DRIFT: EffectDef = EffectDef {
    factory: tape_drift,
    params: Parameterized {
        name: "tape_drift",
        cc_params: Some(Cow::Borrowed(&[CcParam {
            value: ParamType::ZeroOneFloat(0.35),
            cc_norm_index: 2,
            name: "wet_amount",
            description: None,
        }])),
        non_cc_params: None,
    },
};

pub fn stereo_pitch_shifter(params: Arc<Parameterized>) -> EffectFunc {
    Box::new(move |state| {
        let grain_frequency = params.get_non_cc_param("grain_frequency").unwrap();
        let pitch_semi_tones = params.get_non_cc_param("pitch").unwrap();
        let amount = params.cc_fx_or_default("amount", state);
        cc_controlled_wet_dry_fx(
            amount,
            to_net(pitch_shifter(pitch_semi_tones, grain_frequency)),
        )
    })
}

#[distributed_slice(EFFECTS)]
static LOFI_PITCH_SHIFTER: EffectDef = EffectDef {
    factory: tape_drift,
    params: Parameterized {
        name: "pitch_shifter",
        cc_params: Some(Cow::Borrowed(&[CcParam {
            value: ParamType::ZeroOneFloat(0.5),
            cc_norm_index: 2,
            name: "wet_amount",
            description: None,
        }])),
        non_cc_params: Some(Cow::Borrowed(&[
            NonCcParam {
                value: ParamType::Float32(50.0),
                name: "grain_frequency",
                description: Some(
                    "the frequency of grain population - lower means lesser quality, higher means better pitch tracking and timbre",
                ),
            },
            NonCcParam {
                value: ParamType::Float32(11.93),
                name: "pitch",
                description: Some("shiting pitch between -12.0 and +12.0 semitones"),
            },
        ])),
    },
};
