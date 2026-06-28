use crate::SharedMidiState;
use crate::common::fundsp::to_net;
use crate::common::params::{LFO, OscillatorType, ParamNode, Parameterized, Switch};
use fundsp::audiounit::AudioUnit;
use std::str::FromStr;

struct Proph6 {
    a_osc1: OscillatorType,
    a_osc2: OscillatorType,
    a_osc3: OscillatorType,
    b_osc1: OscillatorType,
    b_osc2: OscillatorType,
    b_osc3: OscillatorType,

    polymod_freq_a: Switch,
    polymod_filter: Switch,

    lfo_mod_freq_ab: Switch,
    lfo_mod_pw_ab: Switch,
    lfo_mod_filter: Switch,
    lfo_shape: LFO,
}

pub fn proph6(state: &SharedMidiState, params: &Parameterized) -> Box<dyn AudioUnit> {
    let lfo_freq = params.sound_cc_or_default("lfo_freq", state);
    let lfo_depth = params.sound_cc_or_default("lfo_freq", state);
    let lfo_string = params
        .get_non_cc_param("lfo_shape")
        .unwrap()
        .value
        .to_string();
    let lfo_node = {
        match LFO::from_str(lfo_string.as_str()).unwrap() {
            LFO::Osc(s) => to_net(lfo_freq * lfo_depth >> s.get_node()),
            LFO::Noise(s) => to_net(s.get_node()),
            LFO::SmoothNoise(s) => to_net(lfo_freq * lfo_depth >> s),
        }
    };
    todo!("implement proph 6 :)")
}
