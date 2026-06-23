use fundsp::audionode::AudioNode;
use fundsp::audiounit::AudioUnit;
use fundsp::net::Net;
use fundsp::numeric_array::generic_array::GenericArray;
use fundsp::prelude64::{Fade, NodeId};
use fundsp::typenum::{Const, ToUInt, U};
use fundsp::{Frame, GenericSequence, Size};
use std::sync::Arc;

type GenericNetFunc<const N: usize> = Arc<dyn Fn([f32; N]) -> Net + Send + Sync>;

/// Generic wrapper for M-output effects (where first 0..<M inputs are mapped for audio) which have only f32 params in their signature.
/// A convenience closure that assembles the effect from an array of N ( M audio inputs + function params controlled by cc)
/// which can then be changed via Net::pipe.
/// This allows for modulation (e.g., cc Shared type) to change the effect on the fly - with the effect being rebuilt only when needed.
/// By convention [0..<M] of the inputs are reserved for audio and the rest of N will be the params, in the order in which the closure expects.
///
/// M = 1 means mono,
/// M = 2 means stereo, etc.
///
/// N signifies the total number of inputs via pipe (>>) while M is the output arity (1 = U1, etc.)
/// ### Example
/// ```
/// let reverb_builder = StaticParamsAudioNodeAdapter::<5>::new(Arc::new(
///  |args: [f32; 5]| {
/// /// args[0], args[1] are audio (ignored here, but still passed through - N being the target input count)
/// reverb_stereo(args[2], args[3], args[4])
/// }
/// ));
/// // 5 total inputs with 2 outputs (Stereo)
/// let reverb_adapter = An(StaticParamsAudioNodeAdapter::<5, 2>::new(reverb_builder));
/// /// all inputs are now piped into the wrapper!
/// ( pass() | pass() | cc_1 | cc_2 |cc_3 ) >> reverb_adapter
/// ```
#[derive(Clone)]
pub struct StaticParamsAudioNodeAdapter<const N: usize, const M: usize> {
    inner: GenericNetFunc<N>,
    effect: Net,
    effects_node_id: NodeId,
    params_state: [f32; N],
    params_temp: [f32; N],
    process_cooldown_counter: usize,
    process_calls_threshold: usize,
}

impl<const N: usize, const M: usize> StaticParamsAudioNodeAdapter<N, M> {
    pub(crate) fn new(inner: GenericNetFunc<N>) -> Self {
        let mut s = StaticParamsAudioNodeAdapter {
            inner,
            params_temp: [0.0; N],
            params_state: [0.0; N],
            effect: Net::new(M, M),
            effects_node_id: NodeId::new(),
            process_cooldown_counter: 0,
            process_calls_threshold: 8000,
        };
        assert!(
            N - M <= M,
            "number of total inputs cannot be the same/lower as the number of outputs!"
        );
        s.effects_node_id = s.effect.chain(Box::new((s.inner)(s.params_temp)));
        s
    }
}
impl<const N: usize, const M: usize> StaticParamsAudioNodeAdapter<N, M>
where
    Const<N>: ToUInt,
    U<N>: Size<f32>,
    Const<M>: ToUInt,
    U<M>: Size<f32>,
{
    fn process_cc_events(&mut self, input: &Frame<f32, U<N>>) {
        let mut new_params = [0.0; N];
        // By convention 0, 1 slots will be stereo audio
        for i in M..N {
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

impl<const N: usize, const M: usize> AudioNode for StaticParamsAudioNodeAdapter<N, M>
where
    Const<N>: ToUInt,
    U<N>: Size<f32>,
    Const<M>: ToUInt,
    U<M>: Size<f32>,
{
    const ID: u64 = 60000 + N as u64 + M as u64;
    type Inputs = U<N>;
    type Outputs = U<M>;

    fn reset(&mut self) {
        self.effect.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.effect.set_sample_rate(sample_rate);
    }

    fn tick(&mut self, input: &Frame<f32, U<N>>) -> Frame<f32, Self::Outputs> {
        let mut output = GenericArray::generate(|_| 0.0f32);
        self.process_cc_events(input);
        // By convention 0, 1 slots will be stereo audio
        self.effect.tick(&input[0..2], &mut output);
        Frame::from(output)
    }
}
