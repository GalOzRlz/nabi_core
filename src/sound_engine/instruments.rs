use crate::common::params::CcNode;
use crate::sound_engine::common::cc_unidirectional_spread_step;
use fundsp::numeric_array::generic_array::GenericArray;
use fundsp::prelude64::*;
use std::cmp::max;
use std::ops::Add;

type SpreadStepNode = An<Pipe<Unop<Pipe<Var, Follow<f64>>, FrameMulScalar<U1>>, Unit<U1, U1>>>;

#[derive(serde::Deserialize)]
pub enum Polarity {
    Positive,
    Negative,
}

impl Polarity {
    pub(crate) fn to_float(&self) -> f32 {
        match self {
            Polarity::Positive => 1.0,
            Polarity::Negative => -1.0,
        }
    }
    pub(crate) fn from_string(string: &str) -> Polarity {
        match string.to_lowercase().as_str() {
            "positive" => Polarity::Positive,
            "negative" => Polarity::Negative,
            _ => panic!("did not provide a proper polarity string! {string} != positive/negative"),
        }
    }
}

/// Create a new plucked string based on damping low pass filter and resonant comb filter (Karplus-Strong variant).
///Inputs
/// # Parameters
/// - `feedback`: Decay per sample (0.0 to 1.0). Higher = longer sustain.
/// - `max_delay_seconds`: Maximum delay time for lowest frequency (defines lowest note).
/// - `excitation_gain`: Volume of initial noise burst from input 2 (0.0 to 1.0).
/// - `damping`: intensity of low pass filter on the feedback line (0.0 to 1.1)
/// - `minimum_damping_frequency`: the lowest frequency of the damping filter in hrz
/// - `polarity`: The comb filters' polarity
///
/// Input 0: Pitch
/// Input 1: Gate
/// Input 2: Excitation
/// Input 3: damping factor (0-1)
#[derive(Clone)]
pub struct CombPluck {
    // Constants
    sample_rate: f32,
    max_delay_samples: usize,
    max_delay_samples_f32: f32,
    inv_max_delay: f32,
    feedback: f32,
    excitation_gain: f32,
    smoothing: f32,
    buffer: Vec<f32>,
    write_pos: usize,
    read_pos_f: f32,
    current_freq: f32,
    target_freq: f32,
    last_gate: f32,
    damping_active: bool,
    minimum_damping_frequency: f32,
    maximum_damping_frequency: f32,
    damping: f32,
    damping_freq: f32,
    g: f32,
    filter_state: f32,
}

impl CombPluck {
    pub fn new(
        feedback: f32,
        max_delay_seconds: f32,
        excitation_gain: f32,
        damping: f32,
        minimum_damping_frequency: f32,
        polarity: Polarity,
    ) -> Self {
        let sample_rate = DEFAULT_SR as f32;
        let max_delay_samples = (max_delay_seconds * sample_rate).ceil() as usize;
        let max_delay_samples = max(max_delay_samples, 2);
        let max_delay_samples_f32 = max_delay_samples as f32;

        let mut s = Self {
            sample_rate,
            max_delay_samples,
            max_delay_samples_f32,
            minimum_damping_frequency,
            maximum_damping_frequency: 8_000.0,
            inv_max_delay: 1.0 / max_delay_samples_f32,
            feedback: feedback.clamp(0.0, 1.0) * polarity.to_float(),
            excitation_gain: excitation_gain.clamp(0.0, 1.0),
            smoothing: 0.05,
            buffer: Vec::with_capacity(max_delay_samples),
            write_pos: 0,
            read_pos_f: 0.0,
            current_freq: 440.0,
            target_freq: 440.0,
            last_gate: 0.0,
            damping_active: false,
            damping: 0.0,
            damping_freq: 0.0,
            g: 0.0,
            filter_state: 0.0,
        };
        s.set_damping(damping);
        s
    }
    pub fn set_damping(&mut self, damping: f32) {
        self.damping_active = damping > 0.0;
        self.damping = damping.clamp(0.0, 1.0);
        self.update_damping();
    }
    fn update_damping(&mut self) {
        self.damping_freq = self.maximum_damping_frequency * (1.0 - self.damping)
            + self.minimum_damping_frequency * self.damping;
        self.g = (-std::f32::consts::TAU * self.damping_freq / self.sample_rate).exp();
    }

    /// fill delay line with secondary noise.
    pub fn init_delay_line(&mut self) {
        if self.buffer.is_empty() {
            self.buffer.resize(self.max_delay_samples, 0.0);
        }
        self.buffer.fill(0.0);
        self.write_pos = 0;
        self.read_pos_f = 0.0;
        self.filter_state = 0.0;
    }

    /// Update the fractional read position based on current frequency.
    fn update_delay_length(&mut self) {
        if self.buffer.is_empty() {
            return;
        }

        // Smooth frequency transition
        self.current_freq =
            self.current_freq * (1.0 - self.smoothing) + self.target_freq * self.smoothing;

        if self.current_freq <= 0.0 {
            return;
        }

        // Desired delay length in samples (fractional)
        let delay_samples = self.sample_rate / self.current_freq;
        let delay_samples = delay_samples.min(self.max_delay_samples_f32);

        // Read position = write position - delay length (circular)
        let raw_read = self.write_pos as f32 - delay_samples;
        // euclidean modulo
        let max = self.max_delay_samples_f32;
        self.read_pos_f = raw_read - max * (raw_read * self.inv_max_delay).floor();
    }

    /// Process one sample through the comb filter.
    fn process_comb(&mut self, excitation: f32) -> Frame<f32, typenum::U1> {
        if self.buffer.is_empty() {
            return [0.0].into();
        }

        // Linear interpolation between two buffer positions
        let read_int = self.read_pos_f.floor();
        let read_frac = self.read_pos_f - read_int;
        let idx1 = read_int as usize % self.max_delay_samples;
        let idx2 = (read_int as usize + 1) % self.max_delay_samples;

        let delayed = self.buffer[idx1] * (1.0 - read_frac) + self.buffer[idx2] * read_frac;

        let output = delayed;
        let filtered = {
            // apply low pass to dampen
            let filtered = delayed * (1.0 - self.g) + self.filter_state * self.g;
            self.filter_state = filtered;
            filtered
        };

        // Apply feedback after filtering
        let write_value = filtered * self.feedback + excitation * self.excitation_gain;
        self.buffer[self.write_pos] = write_value;

        // Advance pointers
        self.write_pos = (self.write_pos + 1) % self.max_delay_samples;
        self.read_pos_f += 1.0;
        if self.read_pos_f >= self.max_delay_samples as f32 {
            self.read_pos_f -= self.max_delay_samples as f32;
        }
        [output].into()
    }
}

impl AudioNode for CombPluck {
    const ID: u64 = 67;
    type Inputs = typenum::U4;
    type Outputs = typenum::U1;

    fn reset(&mut self) {
        self.buffer.clear();
        self.buffer.resize(self.max_delay_samples, 0.0);
        self.write_pos = 0;
        self.read_pos_f = 0.0;
        self.current_freq = 440.0;
        self.target_freq = 440.0;
        self.last_gate = 0.0;
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate as f32;
        let duration_secs = self.max_delay_samples_f32 / self.sample_rate;
        self.max_delay_samples = (duration_secs * self.sample_rate).ceil() as usize;
        self.max_delay_samples = Num::max(self.max_delay_samples, 2);
        self.reset();
        self.update_damping();
    }

    #[inline]
    fn tick(&mut self, input: &Frame<f32, Self::Inputs>) -> Frame<f32, Self::Outputs> {
        self.target_freq = input[0].max(0.0);
        let gate = input[1];
        let excitation = input[2].clamp(-1.0, 1.0);
        let damping = input[3].clamp(0.001, 1.0);
        if damping != self.damping {
            self.set_damping(damping);
        }
        // Gate rising edge detection (0→1 transition)
        if self.last_gate <= 0.5 && gate > 0.5 {
            self.init_delay_line();
        }
        self.last_gate = gate;

        if self.target_freq > 0.0 {
            self.update_delay_length();
        }

        let output = self.process_comb(excitation);
        output.into()
    }
}

fn pluck_generic(
    feedback: f32,
    max_delay_seconds: f32,
    excitation_gain: f32,
    damping: f32,
    polarity: Polarity,
) -> An<CombPluck> {
    let max_delay_seconds = max_delay_seconds.clamp(0.0, 2.0);
    An(CombPluck::new(
        feedback,
        max_delay_seconds,
        excitation_gain,
        damping,
        40.0,
        polarity,
    ))
}

pub fn pluck_comb_string(polarity: Polarity) -> An<CombPluck> {
    pluck_generic(0.997, 1.0, 1.4, 0.01, polarity)
}

#[derive(Clone)]
pub struct SuperOSC {
    node: Box<dyn AudioUnit>,
    node_pool: Vec<Box<dyn AudioUnit>>,
    active_node_id_vec: Vec<NodeId>,
    summing_net: Net,
    max_spread_hz: f32,
    detune_spread: CcNode,
    spread_step: SpreadStepNode,
    output_buffer: GenericArray<f32, U1>,
}
impl SuperOSC {
    pub fn new(
        node: Box<dyn AudioUnit>,
        detune_spread: CcNode,
        max_voices: usize,
        max_spread_hz: f32,
    ) -> Self {
        let spread_hz = detune_spread.clone() * max_spread_hz;
        let summing_net = Net::new(0, 1);
        let node_pool = Vec::with_capacity(max_voices);
        let active_node_id_vec = Vec::with_capacity(max_voices);
        let spread_step =
            spread_hz.clone() >> cc_unidirectional_spread_step(max_spread_hz, max_voices);

        let mut s = SuperOSC {
            node,
            node_pool,
            active_node_id_vec,
            summing_net,
            max_spread_hz,
            spread_step,
            detune_spread,
            output_buffer: GenericArray::generate(|_| 0.0),
        };
        s.populate_node_pool();
        s
    }

    pub fn set_voice_count_target(&mut self, voice_count: usize) {
        if voice_count < 3 || voice_count == self.active_node_id_vec.len() {
            return;
        }
        self.rebuild_spread_step(voice_count);
        if voice_count > self.active_node_id_vec.len() {
            let diff = voice_count - self.active_node_id_vec.len();
            self.add_voices_to_summing_net(diff);
        } else if voice_count < self.active_node_id_vec.len() {
            let diff = self.active_node_id_vec.len() - voice_count;
            self.remove_voices_from_summing_net(diff);
        }
    }
    fn rebuild_spread_step(&mut self, voice_count: usize) {
        let spread_hz = self.get_spread_hz();
        self.spread_step =
            spread_hz.clone() >> cc_unidirectional_spread_step(self.max_spread_hz, voice_count);
    }

    fn populate_node_pool(&mut self) {
        for num in 0..self.node_pool.capacity() {
            let step_val = -dc(self.max_spread_hz) + (self.spread_step.clone() * num as f32);
            let processed_node = pass().add(step_val) >> unit::<U1, U1>(self.node.clone());
            self.node_pool.push(Box::new(processed_node));
        }
    }

    fn remove_voices_from_summing_net(&mut self, count: usize) {
        for _ in 0..count {
            if let Some(id_to_remove) = self.active_node_id_vec.pop() {
                let node = unit::<U0, U1>(self.summing_net.remove(id_to_remove));
                self.node_pool.push(Box::new(node))
            }
        }
    }

    fn add_voices_to_summing_net(&mut self, count: usize) {
        for _ in 0..count {
            if let Some(osc) = self.node_pool.pop() {
                let new_id = self.summing_net.fade_in(Fade::Power, 0.008, osc);
                self.active_node_id_vec.push(new_id);
            }
        }
    }
    fn get_spread_hz(&self) -> An<Unop<Pipe<Var, Follow<f64>>, FrameMulScalar<U1>>> {
        self.detune_spread.clone() * self.max_spread_hz
    }
}

impl AudioNode for SuperOSC {
    const ID: u64 = 939999;
    type Inputs = U2;
    type Outputs = U1;

    fn reset(&mut self) {
        self.summing_net.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.summing_net.set_sample_rate(sample_rate);
    }

    fn tick(&mut self, input: &Frame<f32, Self::Inputs>) -> Frame<f32, Self::Outputs> {
        self.set_voice_count_target(input[1] as usize);
        self.summing_net.tick(&[input[0]], &mut self.output_buffer);
        Frame::from(self.output_buffer.clone())
    }
}
