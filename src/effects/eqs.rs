use fundsp::combinator::An;
use fundsp::net::Net;
use fundsp::prelude32::Var;
use fundsp::prelude64::{
    U5, bell_hz, butterpass, constant, db_amp, follow, lowpass_q, pass, pipei, product,
};

pub fn simple_lowpass(cutoff_val: An<Var>, max_cutoff_hz: f32) -> Net {
    let cutoff_hrz = product(constant(max_cutoff_hz), cutoff_val);
    Net::wrap(Box::new(
        (pass() | cutoff_hrz >> follow(0.05_f32)) >> lowpass_q(2.0),
    ))
}

pub fn prophet_lowpass_filter() -> Net {
    Net::wrap(Box::new(!butterpass() >> butterpass()))
}

fn eq5() {
    let mut equalizer =
        pipei::<U5, _, _>(|i| bell_hz(1000.0 + 1000.0 * i as f32, 1.0, db_amp(0.0)));
}
