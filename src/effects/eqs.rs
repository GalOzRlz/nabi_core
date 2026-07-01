use crate::common::params::CcNode;
use fundsp::audionode::Chain;
use fundsp::combinator::An;
use fundsp::net::Net;
use fundsp::prelude64::{
    BellMode, FixedSvf, Tanh, U5, bell_hz, constant, db_amp, dlowpass, follow, lowpass_q, pass,
    pipei, product,
};

pub fn simple_lowpass(cutoff_val: CcNode, max_cutoff_hz: f32) -> Net {
    let cutoff_hrz = product(constant(max_cutoff_hz), cutoff_val);
    Net::wrap(Box::new(
        (pass() | cutoff_hrz >> follow(0.05_f32)) >> lowpass_q(2.0),
    ))
}

pub fn prophet_lowpass_filter() -> Net {
    Net::wrap(Box::new(!dlowpass(Tanh(1.0)) >> !dlowpass(Tanh(1.0))))
}

fn eq5() -> An<Chain<U5, FixedSvf<f64, BellMode<f64>>>> {
    pipei::<U5, _, _>(|i| bell_hz(500.0 + 2000.0 * i as f32, 1.0, db_amp(0.0)))
}
