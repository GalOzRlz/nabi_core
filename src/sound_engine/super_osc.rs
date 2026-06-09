use crate::SharedMidiState;
use crate::common::params::ParamType::U8;
use crate::common::params::Parameterized;
use crate::helpers::fundsp::to_net;
use fundsp::audiounit::AudioUnit;
use fundsp::net::Net;

pub fn super_osc(state: &SharedMidiState, params: &Parameterized) -> Box<dyn AudioUnit> {
    let spread_hz = params.cc_sound_or_default("detune_spread", state) * 100.0;
    let osc = params.get_osc_node_type("osc").unwrap().get_osc_node();
    let osc_count = {
        match params.get_non_cc_param("osc_count").unwrap().value {
            U8(x) => x,
            _ => panic!("osc_count value must be U8"),
        }
    };
    let mut init_net = Net::new(1, 1);
    // todo: use a map to divide the spread to correct unit
    // let detune_unit =
    for num in 0..osc_count {
        init_net = Net::sum(init_net, Net::pipe(state.bent_pitch(), to_net(osc.clone())))
    }
    let synth = Box::new(init_net);
    state.assemble_pitched_sound(synth, params.boxed_adsr("adsr", state))
}
