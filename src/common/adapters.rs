use fundsp::audionode::AudioNode;
use fundsp::audiounit::AudioUnit;
use fundsp::net::Net;
use fundsp::typenum::{Const, ToUInt, U, U2};
use fundsp::{Frame, Size};
use std::sync::Arc;

type GenericStereoToN<const N: usize> = Arc<dyn Fn([f32; N]) -> Net + Send + Sync>;

/// Generic wrapper for stereo effects (where input 0 and 1 are mapped for audio) which have only f32 params in their signature.
/// A convenience closure that assembles the effect from an array of N ( 2 audio inputs + function params controlled by cc)
/// which can then be changed via Net::pipe.
/// This allows for modulation (e.g., cc Shared type) to change the effect on the fly - with the effect being rebuilt only when needed.
/// By convention 0,1 inputs are audio and the rest will be the params, in the order that the closure expects them to appear.
///
/// ### Example
/// ```
/// let full_cc_reverb = StereoStaticParamsWrapper::<6>::new(Arc::new(
/// move |args: [f32; 6]| {
/// /// args[0], args[1] are audio (ignored here, but still passed through)
/// reverb_stereo(args[2], args[3], args[4], args[5])
/// }
/// ));
/// /// all inputs are now piped into the wrapper!
/// ( pass() | pass() | cc_1 | cc_2 |cc_3 | cc_4) >> full_cc_reverb
/// ```
#[derive(Clone)]
pub struct StereoStaticParamsWrapper<const N: usize> {
    inner: GenericStereoToN<N>,
    effect: Net,
    params_state: [f32; N],
    params_temp: [f32; N],
}

impl<const N: usize> StereoStaticParamsWrapper<N> {
    fn new(inner: GenericStereoToN<N>) -> Self {
        StereoStaticParamsWrapper {
            inner,
            params_temp: [0.0; N],
            params_state: [0.0; N],
            effect: Net::new(2, 2),
        }
    }
}
impl<const N: usize> StereoStaticParamsWrapper<N>
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

impl<const N: usize> AudioNode for StereoStaticParamsWrapper<N>
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

    fn tick(&mut self, input: &Frame<f32, U<N>>) -> Frame<f32, U2> {
        let mut output = [0.0f32; 2];
        self.process_cc_events(input);
        // By convention 0, 1 slots will be stereo audio
        self.effect.tick(&input[0..2], &mut output);
        Frame::from(output).into()
    }
}
