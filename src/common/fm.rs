use crate::SharedMidiState;
use crate::common::waveshapers::rectify;
use fundsp::audiounit::Unit;
use fundsp::combinator::An;
use fundsp::net::Net;
use fundsp::prelude::constant;
use fundsp::prelude64::U1;

/// Classic Fm Operator with a modulator and a carrier
pub struct FmOperator {
    pub modulator: An<Unit<U1, U1>>,
    pub carrier: An<Unit<U1, U1>>,
    pub ratio: Net,
    pub amount: Net,
}

impl FmOperator {
    pub fn build_operator_static_fb(self, mod_feedback: f32, state: &SharedMidiState) -> Net {
        let modulator_w_feedback = (state.bent_pitch() * self.ratio)
            >> self.modulator.clone() * constant(mod_feedback)
            >> rectify() * 2.0;
        modulator_w_feedback * (state.bent_pitch() * self.amount.clone()) + state.bent_pitch()
            >> self.carrier.clone()
    }
    pub fn build_operator(self, state: &SharedMidiState) -> Net {
        let modulator = (state.bent_pitch() * self.ratio) >> self.modulator.clone();
        modulator * (state.bent_pitch() * self.amount.clone()) + state.bent_pitch()
            >> self.carrier.clone()
    }
}
