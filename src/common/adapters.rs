use fundsp::audionode::AudioNode;
use fundsp::audiounit::AudioUnit;
use fundsp::net::Net;
use fundsp::numeric_array::generic_array::arr;
use fundsp::prelude64::{Fade, NodeId};
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
/// let reverb_builder = StereoStaticParamsWrapper::<5>::new(Arc::new(
///  |args: [f32; 6]| {
/// /// args[0], args[1] are audio (ignored here, but still passed through - N being the target input count)
/// reverb_stereo(args[2], args[3], args[4])
/// }
/// ));
/// let reverb_adapter = An(StereoStaticParamsWrapper::new(reverb_builder));
/// /// all inputs are now piped into the wrapper!
/// ( pass() | pass() | cc_1 | cc_2 |cc_3 ) >> reverb_adapter
/// ```
#[derive(Clone)]
pub struct StereoStaticParamsWrapper<const N: usize> {
    inner: GenericStereoToN<N>,
    effect: Net,
    effects_node_id: NodeId,
    params_state: [f32; N],
    params_temp: [f32; N],
    process_cooldown_counter: usize,
    process_calls_threshold: usize,
}

impl<const N: usize> StereoStaticParamsWrapper<N> {
    pub(crate) fn new(inner: GenericStereoToN<N>) -> Self {
        let mut s = StereoStaticParamsWrapper {
            inner,
            params_temp: [0.0; N],
            params_state: [0.0; N],
            effect: Net::new(2, 2),
            effects_node_id: NodeId::new(),
            process_cooldown_counter: 0,
            process_calls_threshold: 8000,
        };
        s.effects_node_id = s.effect.chain(Box::new((s.inner)(s.params_temp)));
        s
    }
}
impl<const N: usize> StereoStaticParamsWrapper<N>
where
    Const<N>: ToUInt,
    U<N>: Size<f32>,
{
    fn process_cc_events(&mut self, input: &Frame<f32, U<N>>) {
        let mut new_params = [0.0; N];
        // By convention 0, 1 slots will be stereo audio
        for i in 2..N {
            new_params[i] = input[i];
        }

        if new_params != self.params_temp {
            self.params_temp = new_params;
            self.process_cooldown_counter = 0;
        } else {
            self.process_cooldown_counter += 1;
        }
        if self.params_temp != self.params_state
            && self.process_calls_threshold <= self.process_cooldown_counter
        {
            // todo: need to check this doesn't drag too much on sbc
            self.effect.crossfade(
                self.effects_node_id,
                Fade::Smooth,
                0.01,
                Box::new((self.inner)(self.params_temp)),
            );
            self.params_state = self.params_temp;
            eprintln!("changed value!!!");
            return;
        }
        self.process_cooldown_counter += 1
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

    fn tick(&mut self, input: &Frame<f32, U<N>>) -> Frame<f32, Self::Outputs> {
        let mut output = arr![0.0f32; U2];
        self.process_cc_events(input);
        // By convention 0, 1 slots will be stereo audio
        self.effect.tick(&input[0..2], &mut output);
        Frame::from(output)
    }
}
