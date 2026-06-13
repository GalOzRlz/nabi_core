use fundsp::audionode::AudioNode;
use fundsp::audiounit::AudioUnit;
use fundsp::combinator::An;
use fundsp::typenum::{U, U2};
use fundsp::{Frame, Size};

type GenericStaticStereo = An<dyn AudioNode<Inputs = U2, Outputs = U2>>;

/// Generic wrapper for stereo effects (input 0,1 is audio) which have only f32 params (e.g., reverb_stereo) in their signature.
/// A convenience function that assembles the effect from an array of N static params is wrapped in this struct.
/// This allows for cc values to change the effect on the fly - with the effect being rebuilt on each cc change.
#[derive(Clone)]
struct StereoFXStaticParamsWrapper<const N: usize> {
    inner: dyn Fn([f32; N]) -> GenericStaticStereo,
    effect: GenericStaticStereo,
    params_state: [f32; N + 2],
    params_temp: [f32; N + 2],
}

impl<const N: usize> StereoFXStaticParamsWrapper<N> {
    fn new(inner: Box<dyn Fn([f32; N]) -> GenericStaticStereo>) -> Self {
        StereoFXStaticParamsWrapper {
            inner,
            params_temp: [0.0; N + 2],
            params_state: [0.0; N + 2],
            effect: inner(),
        }
    }
}
impl<const N: usize> StereoFXStaticParamsWrapper<N> {
    fn process_cc_events(&mut self, input: &Frame<f32, Self::Inputs>) {
        // By convention 0, 1 slots will be stereo audio
        for i in 2..N + 2 {
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
    U<N>: Size<f32>,
{
    const ID: u64 = 60000;
    type Inputs = U<{ N + 2 }>;
    type Outputs = U2;
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.inner.set_sample_rate(sample_rate);
    }

    fn tick(&mut self, input: &Frame<f32, Self::Inputs>) -> Frame<f32, Self::Outputs> {
        self.process_cc_events(input);
        // By convention 0, 1 slots will be stereo audio
        self.effect.tick(&input[0..2].into())
    }
}
