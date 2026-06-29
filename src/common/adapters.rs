use fastrand::usize;
use fundsp::audionode::AudioNode;
use fundsp::audiounit::AudioUnit;
use fundsp::net::Net;
use fundsp::numeric_array::generic_array::GenericArray;
use fundsp::prelude64::{Fade, NodeId};
use fundsp::typenum::{Const, ToUInt, U};
use fundsp::{Frame, GenericSequence, Size};
use std::sync::Arc;

type GenericNetFunc<const N: usize> = Arc<dyn Fn([f32; N]) -> Net + Send + Sync>;
type RebuildChangeFn<const N: usize> = dyn Fn([f32; N], [f32; N]) -> bool + Send + Sync;
type RebuildConditionFn<const N: usize> = dyn Fn([f32; N]) -> bool + Send + Sync;

/// Generic wrapper for M-inputs M-outputs Nets (where first 0..<M inputs are mapped for the tick() function) which have only f32 params in their signature.
/// A convenience closure that assembles the net from an array of N ( M audio outputs + static parameters)
/// is provided - which can then be changed via Net::pipe (usually for CC control of static parameters).
/// This allows for modulation of otherwise static parameters on the fly - with the net being rebuilt only when needed (with cooldowning).
/// By convention [0..<M] of the inputs are reserved for audio and the rest of N will be the params, in the order in which the closure expects.
///
/// M = 1 means mono Net,
/// M = 2 means stereo Net, etc.
///
/// N signifies the total number of inputs via pipe (>>) while M is the output arity (1 = U1, etc.)
/// ### Example
/// ```
/// let reverb_builder = StaticParamsAudioNodeAdapter::<5, 2>::new(Arc::new(
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
pub struct StaticParamsAudioNodeAdapter<const N: usize, const M: usize>
where
    Const<N>: ToUInt,
    U<N>: Size<f32>,
    Const<M>: ToUInt,
    U<M>: Size<f32>,
{
    inner: GenericNetFunc<N>,
    net: Net,
    nets_node_id: NodeId,
    params_state: [f32; N],
    params_temp_cooldown: [f32; N],
    params_post_cooldown: [f32; N],
    process_cooldown_counter: usize,
    process_calls_threshold: usize,
    fadeout: bool,
    fadeout_sec: f32,
    rebuild_condition_func: Option<Arc<RebuildConditionFn<N>>>,
    rebuild_change_func: Option<Arc<RebuildChangeFn<N>>>,
    init_checker: bool,
    output_buffer: GenericArray<f32, U<M>>,
    detection_lower_value: usize,
}

impl<const N: usize, const M: usize> StaticParamsAudioNodeAdapter<N, M>
where
    Const<N>: ToUInt,
    U<N>: Size<f32>,
    Const<M>: ToUInt,
    U<M>: Size<f32>,
{
    pub(crate) fn new(inner: GenericNetFunc<N>) -> Self {
        assert!(
            N >= M,
            "number of total inputs cannot be lower than the the number of outputs!"
        );
        let detection_lower_value = { if M == 1 { 0 } else { M } };

        StaticParamsAudioNodeAdapter {
            inner,
            params_post_cooldown: [0.0; N],
            params_temp_cooldown: [0.0; N],
            params_state: [0.0; N],
            net: Net::new(M, M),
            nets_node_id: NodeId::new(),
            process_cooldown_counter: 0,
            process_calls_threshold: 512 * 3,
            fadeout: true,
            fadeout_sec: 0.1,
            rebuild_condition_func: None,
            rebuild_change_func: None,
            init_checker: true,
            output_buffer: GenericArray::generate(|_| 0.0),
            detection_lower_value,
        }
    }

    /// Rebuild the inner function after a stabilization period when the supplied function returns true.
    /// The function is fed the current input stream.
    pub fn rebuild_on_condition<F>(&mut self, func: F)
    where
        F: Fn([f32; N]) -> bool + Send + Sync + 'static,
    {
        self.rebuild_condition_func = Some(Arc::new(func));
    }

    /// Rebuild the inner function after a stabilization-cooldown period when the supplied function returns true.
    /// The function is fed the current input stream and the latest state before the cooldown counter started.
    pub fn rebuild_on_change<F>(&mut self, func: F)
    where
        F: Fn([f32; N], [f32; N]) -> bool + Send + Sync + 'static,
    {
        self.rebuild_change_func = Some(Arc::new(func));
    }

    fn should_rebuild(&self) -> bool {
        if let Some(ref cond) = self.rebuild_condition_func {
            cond(self.params_temp_cooldown)
        } else if let Some(ref change) = self.rebuild_change_func {
            change(self.params_temp_cooldown, self.params_state)
        } else {
            true
        }
    }

    pub fn set_fadeout_time(&mut self, fadeout_sec: f32) {
        self.fadeout_sec = fadeout_sec;
        self.enable_fadeout()
    }

    pub fn disable_fadeout(&mut self) {
        self.fadeout = false;
    }

    pub fn enable_fadeout(&mut self) {
        self.fadeout = true;
    }

    pub fn set_cooldown_samples(&mut self, samples: usize) {
        self.process_calls_threshold = samples;
    }

    fn process_cc_events(&mut self, input: &Frame<f32, U<N>>) {
        for i in 0..self.params_state.len() {
            self.params_temp_cooldown[i] = input[i];
        }
        if self.params_temp_cooldown[self.detection_lower_value..N]
            != self.params_post_cooldown[self.detection_lower_value..N]
        {
            self.params_post_cooldown = self.params_temp_cooldown;
            self.process_cooldown_counter = 0;
        } else {
            self.process_cooldown_counter += 1;
        }
        if self.params_post_cooldown[self.detection_lower_value..N]
            != self.params_state[self.detection_lower_value..N]
            && self.process_calls_threshold <= self.process_cooldown_counter
        {
            if self.should_rebuild() {
                println!("rebuilding as new value");
                let fadeout = if self.fadeout {
                    self.fadeout_sec
                } else {
                    0.001
                };
                self.net.crossfade(
                    self.nets_node_id,
                    Fade::Power,
                    fadeout,
                    Box::new((self.inner)(self.params_post_cooldown)),
                );
                self.params_state = self.params_temp_cooldown;
                eprintln!("changed value!!!");
            } else {
                self.process_cooldown_counter += 1
            }
        }
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
        self.net.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.net.set_sample_rate(sample_rate);
    }

    fn tick(&mut self, input: &Frame<f32, U<N>>) -> Frame<f32, Self::Outputs> {
        // initialize with actual values on first tick()
        if self.init_checker {
            self.init_checker = false;
            self.params_state = input.as_slice().try_into().unwrap();
            self.nets_node_id = self.net.chain(Box::new((self.inner)(self.params_state)));
        } else {
            self.process_cc_events(input);
        }
        self.net.tick(&input[0..M], &mut self.output_buffer);
        Frame::from(self.output_buffer.clone())
    }
}
