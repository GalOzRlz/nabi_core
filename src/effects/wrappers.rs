use fundsp::audionode::AudioNode;
use fundsp::audiounit::AudioUnit;
use fundsp::net::Net;
use fundsp::typenum::{Const, ToUInt, U, U2};
use fundsp::{Frame, Size};
use std::sync::Arc;

/// Generic wrapper for stereo effects (input 0,1 is audio) which have only f32 params (e.g., reverb_stereo) in their signature.
/// A convenience function that assembles the effect from an array of N static params is wrapped in this struct.
/// This allows for cc values to change the effect on the fly - with the effect being rebuilt on each cc change.
#[derive(Clone)]
struct StereoFXStaticParamsWrapper<const N: usize> {
    inner: Arc<dyn Fn([f32; N]) -> Net + Send + Sync>,
    effect: Net,
    params_state: [f32; N],
    params_temp: [f32; N],
}

impl<const N: usize> StereoFXStaticParamsWrapper<N> {
    fn new(inner: Arc<dyn Fn([f32; N]) -> Net + Send + Sync>) -> Self {
        StereoFXStaticParamsWrapper {
            inner,
            params_temp: [0.0; N],
            params_state: [0.0; N],
            effect: Net::new(2, 2),
        }
    }
}
impl<const N: usize> StereoFXStaticParamsWrapper<N>
where
    Const<N>: ToUInt,
    U<N>: Size<f32>,
{
    fn process_cc_events(&mut self, input: &Frame<f32, U<N>>) {
        // By convention 0, 1 slots will be stereo audio
        for i in 2..N {
            self.params_temp[i] = input[i];
        }
        if self.params_temp != self.params_state {
            self.effect = (self.inner)(self.params_temp);
            self.params_state = self.params_temp;
        };
    }
}

impl<const N: usize> AudioNode for StereoFXStaticParamsWrapper<N>
where
    Const<N>: ToUInt,
    U<N>: Size<f32>,
{
    const ID: u64 = 60000;
    type Inputs = U<N>;
    type Outputs = U2;
    fn reset(&mut self) {
        self.effect.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.effect.set_sample_rate(sample_rate);
    }

    fn tick(&mut self, input: &Frame<f32, U<N>>) -> Frame<f32, Self::Outputs> {
        let mut output = [0.0f32; 2];
        self.process_cc_events(input);
        // By convention 0, 1 slots will be stereo audio
        self.effect.tick(&input[0..2], &mut output);
        Frame::from(output).into()
    }
}
