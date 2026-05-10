use fundsp::combinator::An;
use fundsp::prelude64::*;

pub fn master_reverb(wet: f32) -> An<Bus<Unop<MultiPass<U2>, FrameMulScalar<U2>>, Unop<impl AudioNode<Inputs=U2, Outputs=U2>, FrameMulScalar<U2>>>> {
    let wet= wet.clamp(0.0, 1.0);
    let dry = 1.0 - wet;
    (multipass() * dry) & (wet * reverb_stereo(5.0, 2.5, 0.5))
}
pub fn simple_lowpass(cutoff_val: An<Var>, max_cutoff_hz: f32) -> An<Pipe<Pipe<Pipe<Stack<Pass, Pipe<Binop<FrameMul<fundsp::typenum::U1>, Constant<typenum::U1>, Var>, Follow<f64>>>, Pipe<Stack<MultiPass<U2>, Constant<U1>>, Svf<f64, LowpassMode<f64>>>>, DCBlock<f64>>, Shaper<Clip>>> {
    let cutoff_hrz = product(constant(max_cutoff_hz / 127.0), cutoff_val);
    (pass() | cutoff_hrz >> follow(0.05_f32)) >> lowpass_q(2.0) >> dcblock() >> clip()
}