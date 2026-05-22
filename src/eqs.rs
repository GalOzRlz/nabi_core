use crate::effects::cc_smooth;
use fundsp::combinator::An;
use fundsp::net::Net;
use fundsp::prelude32::Var;
use fundsp::prelude64::{
    U2, butterpass, constant, follow, highpass_q, lowpass_q, moog_q, multipass, pass, product, var,
};

pub fn simple_lowpass(cutoff_val: An<Var>, max_cutoff_hz: f32) -> Net {
    let cutoff_hrz = product(constant(max_cutoff_hz), cutoff_val) >> cc_smooth();
    Net::wrap(Box::new(
        (pass() | cutoff_hrz >> follow(0.05_f32)) >> lowpass_q(2.0),
    ))
}

pub fn prophet_lowpass_filter() -> Net {
    Net::wrap(Box::new(!butterpass() >> butterpass()))
}
