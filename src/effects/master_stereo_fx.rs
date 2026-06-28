use crate::common::adapters::StaticParamsAudioNodeAdapter;
use crate::common::fundsp::to_net;
use crate::common::params::{CcAudioNode, CcParam, NonCcParam, ParamType, Parameterized};
use crate::effects::effects_building::EffectFunc;
use crate::effects::effects_building::{EFFECTS, EffectDef};
use crate::effects::helpers::cc_controlled_wet_dry_fx;
use crate::effects::pitch_modulation::{pitch_shifter, stereo_j_chorus, tape_wow};
use fundsp::prelude64::*;
use linkme::distributed_slice;
use std::borrow::Cow;
use std::sync::Arc;

pub fn master_limiter() -> Net {
    let master = multipass::<U2>() >> limiter_stereo(0.012, 0.3);
    to_net(master)
}

fn cc_controlled_reverb(
    wet_amount: CcAudioNode,
    reverb_time: CcAudioNode,
    room_size: CcAudioNode,
    damping: CcAudioNode,
) -> Net {
    let reverb_builder = Arc::new(|x: [f32; 5]| (to_net(reverb_stereo(x[2], x[3], x[4]))));
    let mut reverb_adapter = StaticParamsAudioNodeAdapter::<5, 2>::new(reverb_builder);
    reverb_adapter.set_fadeout_time(0.5);
    let reverb =
    // assumes room size and reverb times are 0-10
        (pass() | pass() | room_size * 10.0 | reverb_time * 10.0 | damping) >> An(reverb_adapter) * 1.3;
    cc_controlled_wet_dry_fx(wet_amount, to_net(reverb))
}

fn fundsp_reverb_factory(params: Arc<Parameterized>) -> EffectFunc {
    Box::new(move |state| {
        let damping_param = params.fx_cc_or_default("damping", state);
        let wet_amount = params.fx_cc_or_default("%", state);

        let room_size_param = params.fx_cc_or_default("room_size", state);
        let length_param = params.fx_cc_or_default("length", state);
        cc_controlled_reverb(wet_amount, length_param, room_size_param, damping_param)
    })
}

#[distributed_slice(EFFECTS)]
static REVERB: EffectDef = EffectDef {
    factory: fundsp_reverb_factory,
    params: Parameterized {
        name: "reverb",
        cc_params: Some(Cow::Borrowed(&[
            CcParam {
                value: ParamType::ZeroOneFloat(0.35),
                cc_norm_index: 1,
                name: "%",
                description: None,
            },
            CcParam {
                value: ParamType::ZeroTenFloat(4.0),
                cc_norm_index: 0,
                name: "room_size",
                description: None,
            },
            CcParam {
                value: ParamType::ZeroOneFloat(0.8),
                cc_norm_index: 0,
                name: "damping",
                description: None,
            },
            CcParam {
                value: ParamType::ZeroTenFloat(2.0),
                cc_norm_index: 0,
                name: "length",
                description: None,
            },
        ])),
        non_cc_params: None,
    },
};

fn eq2(params: Arc<Parameterized>) -> EffectFunc {
    Box::new(move |state| {
        let lowpass_cutoff = params.fx_cc_or_default("highcut_freq", state);
        let lowpass_q = params.fx_cc_or_default("lowpass_q", state);
        let highpass_cutoff = params.fx_cc_or_default("lowcut_freq", state);
        let highpass_q = params.fx_cc_or_default("highpass_q", state);

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
                value: ParamType::ZeroOneFloat(0.0),
                cc_norm_index: 3,
                name: "lowcut_freq",
                description: None,
            },
            CcParam {
                value: ParamType::ZeroOneFloat(0.05),
                cc_norm_index: 0,
                name: "lowpass_q",
                description: None,
            },
            CcParam {
                value: ParamType::ZeroOneFloat(1.0),
                cc_norm_index: 4,
                name: "highcut_freq",
                description: None,
            },
            CcParam {
                value: ParamType::ZeroOneFloat(0.35),
                cc_norm_index: 0,
                name: "highpass_q",
                description: None,
            },
        ])),
        non_cc_params: Some(Cow::Borrowed(&[
            NonCcParam {
                value: ParamType::Float32(18_000.0),
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
        let depth_net = params.fx_cc_or_default("depth", state);
        tape_wow(depth_net)
    })
}

#[distributed_slice(EFFECTS)]
static TAPE_DRIFT: EffectDef = EffectDef {
    factory: tape_drift,
    params: Parameterized {
        name: "drift",
        cc_params: Some(Cow::Borrowed(&[CcParam {
            value: ParamType::ZeroOneFloat(0.35),
            cc_norm_index: 2,
            name: "depth",
            description: None,
        }])),
        non_cc_params: None,
    },
};

pub fn stereo_pitch_shifter(params: Arc<Parameterized>) -> EffectFunc {
    Box::new(move |state| {
        let grain_frequency = params.get_non_cc_param("grain_frequency").unwrap();
        let pitch_semi_tones = params.get_non_cc_param("pitch").unwrap();
        let amount = params.fx_cc_or_default("%", state);
        cc_controlled_wet_dry_fx(
            amount,
            to_net(pitch_shifter(pitch_semi_tones, grain_frequency)),
        )
    })
}

#[distributed_slice(EFFECTS)]
static LOFI_PITCH_SHIFTER: EffectDef = EffectDef {
    factory: stereo_pitch_shifter,
    params: Parameterized {
        name: "pitch_shifter",
        cc_params: Some(Cow::Borrowed(&[CcParam {
            value: ParamType::ZeroOneFloat(0.5),
            cc_norm_index: 2,
            name: "%",
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

pub fn j_chorus(params: Arc<Parameterized>) -> EffectFunc {
    Box::new(move |state| {
        let depth = params.fx_cc_or_default("depth", state);
        let mod_frequency = params.fx_cc_or_default("mod_freq", state);
        stereo_j_chorus(depth, mod_frequency)
    })
}

#[distributed_slice(EFFECTS)]
static J_CHORUS: EffectDef = EffectDef {
    factory: j_chorus,
    params: Parameterized {
        name: "j_chorus",
        cc_params: Some(Cow::Borrowed(&[
            CcParam {
                value: ParamType::ZeroOneFloat(0.8),
                cc_norm_index: 2,
                name: "depth",
                description: None,
            },
            CcParam {
                value: ParamType::ZeroTenFloat(0.822), // Mode II on the Juno-60 (Mode I is around  0.5, III is  9.425)
                cc_norm_index: 3,
                name: "mod_freq",
                description: None,
            },
        ])),
        non_cc_params: None,
    },
};
