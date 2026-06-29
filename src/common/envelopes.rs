use crate::GATE_OFF;
use crate::common::adapters::StaticParamsAudioNodeAdapter;
use crate::common::fundsp::to_net;
use crate::common::params::{CcNode, CcParam, ParamType};
use fundsp::audionode::{Pass, Pipe, Stack};
use fundsp::follow::Follow;
use fundsp::prelude::adsr_live;
use fundsp::prelude32::Var;
use fundsp::prelude64::{An, pass};
use std::sync::Arc;

pub type CcADSR = An<
    Pipe<
        Stack<
            Stack<
                Stack<Stack<Pass, Pipe<Var, Follow<f64>>>, Pipe<Var, Follow<f64>>>,
                Pipe<Var, Follow<f64>>,
            >,
            Pipe<Var, Follow<f64>>,
        >,
        StaticParamsAudioNodeAdapter<5, 1>,
    >,
>;

pub fn cc_controlled_adsr_params(
    attack_cc_idx: usize,
    decay_cc_idx: usize,
    sustain_cc_idx: usize,
    release_cc_idx: usize,
) -> [CcParam; 4] {
    [
        CcParam {
            value: ParamType::ZeroTenFloat(0.005),
            cc_norm_index: attack_cc_idx,
            name: "attack",
            description: Some("attack rate: with CC goes from 0.0 to 5 seconds"),
        },
        CcParam {
            value: ParamType::ZeroTenFloat(0.1),
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
            value: ParamType::ZeroTenFloat(0.2),
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

pub fn assemble_cc_adsr(a: CcNode, d: CcNode, s: CcNode, r: CcNode) -> CcADSR {
    let mut cc_adsr = cc_controlled_adsr();
    cc_adsr.rebuild_on_condition(|x| x[0] == GATE_OFF);
    cc_adsr.disable_fadeout();
    (pass() | a | d | s | r) >> cc_adsr
}

macro_rules! define_adsr_params {
    (
        $(
            $name:ident : cc_idx = $cc_idx:expr, default = $default:expr
        ),*
        $(,)?
    ) => {
        [
            $(
                {
                    // Fixed description based on parameter name
                    let desc = match stringify!($name) {
                        "attack"  => "attack rate: with CC goes from 0.0 to 5 seconds",
                        "decay"   => "decay rate: with CC goes from 0.0 to 5 seconds",
                        "sustain" => "sustain level from 0.0 to 1.0",
                        "release" => "decay rate: with CC goes from 0.0 to 5 seconds",
                        _ => panic!("Unknown parameter name: {}", stringify!($name)),
                    };

                    // Choose the right ParamType variant
                    let value = match stringify!($name) {
                        "sustain" => ParamType::ZeroOneFloat($default),
                        _         => ParamType::ZeroTenFloat($default),
                    };

                    CcParam {
                        value,
                        cc_norm_index: $cc_idx,
                        name: stringify!($name),
                        description: Some(desc),
                    }
                }
            ),*
        ]
    };
}
