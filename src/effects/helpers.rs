use fundsp::audiounit::AudioUnit;
use fundsp::net::Net;
use fundsp::prelude64::{U2, constant, multipass};

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
