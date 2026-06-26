use crate::common::adapters::StaticParamsAudioNodeAdapter;
use crate::common::fundsp::to_net;
use crate::common::params::{CcParam, ParamType};
use fundsp::prelude::adsr_live;
use fundsp::prelude64::An;
use std::sync::Arc;

pub fn cc_controlled_adsr_params(
    attack_cc_idx: usize,
    decay_cc_idx: usize,
    sustain_cc_idx: usize,
    release_cc_idx: usize,
) -> [CcParam; 4] {
    [
        CcParam {
            value: ParamType::Float32(0.005),
            cc_norm_index: attack_cc_idx,
            name: "attack",
            description: Some("attack rate: with CC goes from 0.0 to 5 seconds"),
        },
        CcParam {
            value: ParamType::Float32(0.1),
            cc_norm_index: decay_cc_idx,
            name: "decay",
            description: Some("decay rate: with CC goes from 0.0 to 5 seconds"),
        },
        CcParam {
            value: ParamType::ZeroOneFloat(1.0),
            cc_norm_index: sustain_cc_idx,
            name: "sustain",
            description: Some("sustain level from 0.0 to 1.0"),
        },
        CcParam {
            value: ParamType::Float32(0.2),
            cc_norm_index: release_cc_idx,
            name: "release",
            description: Some("decay rate: with CC goes from 0.0 to 5 seconds"),
        },
    ]
}

/// ADSR envelope that recieves the following inputs:
/// Input 0: Gate value
/// Input 1: Attack (Seconds)
/// Input 2: Decay (Seconds)
/// Input 3: Sustain (0.0-1.0 gain)
/// Input 4: Release (Seconds0]
///
/// Output 0: scaled ADSR value from 0.0 to 1.0
pub fn cc_controlled_adsr() -> An<StaticParamsAudioNodeAdapter<5, 1>> {
    An(StaticParamsAudioNodeAdapter::<5, 1>::new(Arc::new(
        |args: [f32; 5]| to_net(adsr_live(args[1], args[2], args[3], args[4])),
    )))
}

pub fn cc_controlled_attack_decay() -> An<StaticParamsAudioNodeAdapter<3, 1>> {
    An(StaticParamsAudioNodeAdapter::<3, 1>::new(Arc::new(
        |args: [f32; 3]| to_net(adsr_live(args[1], args[2], 0.0, 0.0)),
    )))
}
