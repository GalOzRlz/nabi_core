use crate::SharedMidiState;
use fundsp::audiounit::{AudioUnit, Unit};
use fundsp::combinator::An;
use fundsp::net::Net;
use fundsp::prelude64::U1;

/// Classic Fm Operator with a modulator and a carrier
pub struct FmConnector {
    pub modulator: An<Unit<U1, U1>>,
    pub carrier: An<Unit<U1, U1>>,
    pub ratio: Net,
    pub amount: Net,
}

impl FmConnector {
    pub fn connect_operators_with_env(
        self,
        modulator_env: Box<dyn AudioUnit>,
        carrier_env: Option<Box<dyn AudioUnit>>,
    ) -> FmConnector {
        todo!("add envelopes to carrier and modulator")
    }
    pub fn connect_operators(self, state: &SharedMidiState) -> Net {
        let modulator = (state.bent_pitch() * self.ratio) >> self.modulator.clone();
        modulator * (state.bent_pitch() * self.amount.clone()) + state.bent_pitch()
            >> self.carrier.clone()
    }
}
