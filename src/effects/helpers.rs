use fundsp::audiounit::AudioUnit;
use fundsp::net::Net;
use fundsp::prelude64::{U2, constant, multipass};
use std::sync::Arc;

/// Factory for stereo effects with wet/dry control via Net  (suitable for live Midi CC)
pub fn cc_controlled_wet_dry_fx(wet_amount: Net, effect: Net) -> Net {
    // Duplicate wet to stereo (0 inputs, 2 outputs)
    let effect = to_stereo(effect);
    let wet_stereo = wet_amount.clone() | wet_amount.clone();

    let dry_mono = constant(1.0) - wet_amount;
    let dry_stereo = dry_mono.clone() | dry_mono;

    let pass = Net::wrap(Box::new(multipass::<U2>())); // U2 -> U2 identity
    (pass * dry_stereo) & (effect * wet_stereo)
}

pub fn to_stereo(net: Net) -> Net {
    match net.inputs() {
        1 => (net.clone() | net),
        2 => net,
        _ => panic!("only 1 and 2 inputs are supported!"),
    }
}

pub fn connect_node_vec(node_vec: Arc<Vec<Net>>, starting_net: Option<Net>) -> Net {
    let nodes = (*node_vec).clone();
    let mut net = starting_net.unwrap_or_else(|| {
        Net::wrap(Box::new(fundsp::prelude::multipass::<fundsp::prelude::U2>()))
    });
    for node in nodes {
        net = to_stereo(net) >> node;
    }
    net
}
