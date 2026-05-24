use crate::config_builder::{FreeVoiceStrategy, GlobalConfig, VoiceStealingConfig};
use crate::effects::master_fx::master_limiter;
use crate::io::midi::PatchButton;
pub use crate::io::midi::SynthMsg;
use crate::patch_builder::KnobGroup;
use crate::sound_engine::sound_building::SynthFunc;
use crate::{NUM_MIDI_VALUES, SharedMidiState, patch_builder::PatchTable};
use anyhow::{anyhow, bail};
use bare_metal_modulo::*;
use cpal::{
    Device, FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig,
    SupportedBufferSize,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use crossbeam_queue::SegQueue;
use fundsp::prelude::{NetBackend, U2};
use fundsp::prelude32::Net;
use fundsp::prelude64::{BufferVec, Fade, NodeId, split};
use fundsp::{
    prelude::AudioUnit,
    prelude64::{shared, var},
    shared::Shared,
};
use midi_msg::ControlChange::CC;
use midi_msg::{Channel, ChannelModeMsg, ChannelVoiceMsg, MidiMsg, SystemRealTimeMsg};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;

struct Buffers {
    output: BufferVec,
    input: BufferVec,
}
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
/// Represents whether a sound should go to the left, right, or both speakers.
pub enum Speaker {
    Left,
    Right,
    Both,
}

impl Speaker {
    /// Value for using a `Speaker` as an array index.
    pub fn i(&self) -> usize {
        *self as usize
    }
}
pub trait Synth<const N: usize> {
    fn new(patch_table: Arc<PatchTable>, config: GlobalConfig) -> Self;

    fn run_output(&mut self, midi_msgs: Arc<SegQueue<SynthMsg>>) -> anyhow::Result<()>;

    fn decode(&mut self, speaker: Speaker, msg: &MidiMsg) -> Option<RelayedMessage>;
    fn run_synth<T: Sample + SizedSample + FromSample<f32>>(
        &mut self,
        midi_msgs: Arc<SegQueue<SynthMsg>>,
        device: Device,
        config: StreamConfig,
    ) -> anyhow::Result<()> {
        Self::warm_up(midi_msgs.clone());
        let stream = self.get_stream::<T>(&config, &device)?;
        stream.play()?;
        while self.handle_messages(midi_msgs.clone()) != RelayedMessage::SystemReset {
            std::thread::sleep(std::time::Duration::from_millis(1000));
        }

        Ok(())
    }

    fn warm_up(midi_msgs: Arc<SegQueue<SynthMsg>>) {
        for _ in 0..N {
            midi_msgs.push(Self::warm_up_msg(ChannelVoiceMsg::NoteOn {
                note: 0,
                velocity: 0,
            }));
            midi_msgs.push(Self::warm_up_msg(ChannelVoiceMsg::NoteOff {
                note: 0,
                velocity: 0,
            }));
        }
    }

    fn warm_up_msg(msg: ChannelVoiceMsg) -> SynthMsg {
        SynthMsg {
            msg: MidiMsg::ChannelVoice {
                channel: Channel::Ch1,
                msg,
            },
            speaker: Speaker::Both,
        }
    }

    fn handle_messages(&mut self, midi_msgs: Arc<SegQueue<SynthMsg>>) -> RelayedMessage {
        loop {
            if let Some(msg) = midi_msgs.pop()
                && let Some(relayed) = self.decode(msg.speaker, &msg.msg)
            {
                return relayed;
            }
        }
    }

    fn get_stream<T>(&mut self, config: &StreamConfig, device: &Device) -> anyhow::Result<Stream>
    where
        T: Sample + FromSample<f32> + SizedSample;
}

/// The default player that has one stereo stream in and one out (U2 inputs, U2 outputs)
pub struct SynthPlayer<const N: usize> {
    voice_manager: VoiceManager<N>,
    buffers: Buffers,
}

impl<const N: usize> Synth<N> for SynthPlayer<N> {
    fn new(patch_table: Arc<PatchTable>, config: GlobalConfig) -> Self {
        let voice_manager = VoiceManager::<N>::new(patch_table.clone(), config);
        Self {
            voice_manager,
            buffers: Buffers {
                output: BufferVec::new(2),
                input: BufferVec::new(2),
            },
        }
    }

    fn run_output(&mut self, midi_msgs: Arc<SegQueue<SynthMsg>>) -> anyhow::Result<()> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(anyhow!("failed to find a default output device"))?;
        let default_config = device.default_output_config().expect("No default config");

        // 2. Query the device's supported buffer size range
        let buffer_size_range = default_config.buffer_size();

        let buffer_size = match buffer_size_range {
            // If the device reports a min/max range, pick a value in between
            SupportedBufferSize::Range { min, max } => {
                let target = 441; // todo: make it configurable?
                // Clamp the target to the valid range [min, max]
                let chosen = target.clamp(*min, *max);
                println!(
                    "Device supports buffer sizes {}-{}. Using {}.",
                    min, max, chosen
                );
                cpal::BufferSize::Fixed(chosen)
            }
            // If the device doesn't report a range, fall back to the default
            SupportedBufferSize::Unknown => {
                println!("Device buffer size range unknown, using default.");
                cpal::BufferSize::Default
            }
        };

        // 4. Build your final stream configuration
        let config = StreamConfig {
            channels: default_config.channels(),
            sample_rate: default_config.sample_rate(),
            buffer_size,
        };
        match default_config.sample_format() {
            SampleFormat::F32 => self.run_synth::<f32>(midi_msgs, device, config.into()),
            SampleFormat::I16 => self.run_synth::<i16>(midi_msgs, device, config.into()),
            SampleFormat::U16 => self.run_synth::<u16>(midi_msgs, device, config.into()),
            sample_format => panic!("Unsupported sample format '{sample_format}'"),
        }
    }

    fn decode(&mut self, _speaker: Speaker, msg: &MidiMsg) -> Option<RelayedMessage> {
        let result = None;
        result.or(self.voice_manager.decode(msg))
    }
    fn get_stream<T>(&mut self, config: &StreamConfig, device: &Device) -> anyhow::Result<Stream>
    where
        T: Sample + FromSample<f32> + SizedSample,
    {
        let sample_rate = config.sample_rate as f64;
        let mut mix = self.voice_manager.mix_net_backend();
        mix.reset();
        mix.set_sample_rate(sample_rate);
        let input_buffer = self.buffers.input.clone();
        let mut output_buffer = self.buffers.output.clone();
        let mut next_block = move |block: &mut [(f32, f32)], n_frames: usize| {
            mix.process(
                n_frames,
                &input_buffer.buffer_ref(),
                &mut output_buffer.buffer_mut(),
            );
            for _ in 0..n_frames {
                for i in 0..n_frames {
                    block[i] = (output_buffer.at_f32(0, i), output_buffer.at_f32(1, i));
                }
            }
        };

        let channels = config.channels as usize;
        let block_size = 64; // FunDSP’s max block size

        let err_fn = |err| eprintln!("Error on stream: {err}");

        device
            .build_output_stream(
                config,
                move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                    write_data_block(data, channels, block_size, &mut next_block);
                },
                err_fn,
                None,
            )
            .or_else(|err| bail!("{err:?}"))
    }
}

pub fn write_data_block<T: Sample + FromSample<f32>>(
    output: &mut [T],
    channels: usize,
    block_size: usize,
    next_block: &mut dyn FnMut(&mut [(f32, f32)], usize),
) {
    let frame_count = output.len() / channels;
    let mut block_buffer = vec![(0.0f32, 0.0f32); block_size];
    let mut frames_written = 0;

    while frames_written < frame_count {
        let remaining = frame_count - frames_written;
        let frames_to_gen = remaining.min(block_size);

        next_block(&mut block_buffer[..frames_to_gen], frames_to_gen);

        // Copy the block into the output buffer
        for (i, (l, r)) in block_buffer[..frames_to_gen].iter().enumerate() {
            let index = (frames_written + i) * channels;
            output[index] = T::from_sample(*l);
            if channels == 2 {
                output[index + 1] = T::from_sample(*r);
            }
        }
        frames_written += frames_to_gen;
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum RelayedMessage {
    SystemReset,
}

/// Single sound emitter that decodes midi and manages voices - used by SynthPlayer and LRPlayer to manage output.
#[derive(Clone)]
struct VoiceManager<const N: usize> {
    states: [SharedMidiState; N],
    next: ModNumC<usize, N>,
    pitch2state: [Option<usize>; NUM_MIDI_VALUES],
    recent_pitches: [Option<u8>; N],
    synth_func: SynthFunc,
    master_volume: Shared,
    patch_table: Arc<PatchTable>,
    config: GlobalConfig,
    fx_node_id: NodeId,
    sound_node_id: NodeId,
    mix_net: Net,
    current_patch_num: usize,
    sound_cc_vals: Vec<f32>,
    fx_cc_vals: Vec<f32>,
    cc_to_knob: HashMap<u8, (KnobGroup, usize)>, // CC → (group, 0‑based index)
}

impl<const N: usize> VoiceManager<N> {
    fn new(patch_table: Arc<PatchTable>, config: GlobalConfig) -> Self {
        let mut cc_to_knob = HashMap::new();
        for (i, &cc) in config.sound_cc_mapping.iter().enumerate() {
            cc_to_knob.insert(cc, (KnobGroup::Sound, i));
        }
        for (i, &cc) in config.fx_cc_mapping.iter().enumerate() {
            cc_to_knob.insert(cc, (KnobGroup::Effect, i));
        }
        let sound_len = config.sound_cc_mapping.len().max(1);
        let effect_len = config.fx_cc_mapping.len().max(1);
        let first_table = &patch_table.clone().entries[0];
        let synth_func = first_table.sound_factory.build();
        let fx_cc_array = &first_table.effects.initial_cc;
        let sound_cc_array = &first_table.sound_factory.initial_cc;
        let tuner = first_table.tuning;
        let mut master_fx_net = Net::new(2, 2);

        let states = [(); N].map(|_| {
            SharedMidiState::new(
                &config.sound_cc_mapping,
                &config.fx_cc_mapping,
                &sound_cc_array,
                &fx_cc_array,
                tuner,
            )
        });

        let fx_node_id =
            master_fx_net.chain(Box::new(first_table.effects.clone().build(&states[0])));
        Self {
            states,
            next: ModNumC::new(0),
            pitch2state: [None; NUM_MIDI_VALUES],
            recent_pitches: [None; N],
            synth_func,
            master_volume: shared(0.15),
            patch_table,
            config: config.clone(),
            sound_cc_vals: vec![0.0; sound_len],
            fx_cc_vals: vec![0.0; effect_len],
            cc_to_knob,
            current_patch_num: 0,
            fx_node_id,
            mix_net: Net::new(2, 2),
            sound_node_id: NodeId::new(),
        }
    }
    fn set_midi_to_hz(&mut self, midi_to_hz: fn(f32) -> f32) {
        for i in 0..self.states.len() {
            self.states[i].set_midi_to_hz(midi_to_hz);
        }
    }

    fn nullify_zero_value_notes(&mut self, sound: &mut Net, i: usize) -> bool {
        let sample = sound.get_mono();
        if sample == 0_f32 {
            self.release(i);
            return true;
        }
        false
    }
    fn mix_net_backend(&mut self) -> NetBackend {
        let backend = self.mix_net.backend();
        let sound = self.sound();
        self.sound_node_id = self.mix_net.chain(Box::new(sound));
        let table = self.patch_table.clone();
        if let Some(entry) = table.entries.get(self.current_patch_num) {
            self.fx_node_id = self
                .mix_net
                .chain(Box::new(entry.effects.clone().build(&self.states[0])));
        }
        self.mix_net.commit();
        backend
    }

    fn sound(&mut self) -> Net {
        let mut sound = Net::wrap(self.sound_at(0));
        if self.config.voice_release == FreeVoiceStrategy::ReleaseOnZero {
            self.nullify_zero_value_notes(&mut sound, 0);
        }
        for i in 1..N {
            sound = sound + Net::wrap(self.sound_at(i));
            if self.config.voice_release == FreeVoiceStrategy::ReleaseOnZero {
                self.nullify_zero_value_notes(&mut sound, i);
            }
        }
        let mix = match sound.outputs() {
            1 => {
                let vol = var(&self.master_volume);
                (sound * vol) >> split::<U2>()
            }
            2 => {
                let vol = var(&self.master_volume);
                sound * vol
            }
            _ => panic!("Unsupported output count on synth! use either U1 (mono) or U2 (stereo)"),
        };
        mix >> master_limiter()
    }

    fn decode(&mut self, msg: &MidiMsg) -> Option<RelayedMessage> {
        match msg {
            MidiMsg::ChannelVoice { channel: _, msg } => match msg {
                ChannelVoiceMsg::NoteOn { note, velocity } => {
                    if *velocity == 0_u8 {
                        self.off(*note);
                    } else {
                        self.on(*note, *velocity);
                    }
                }
                ChannelVoiceMsg::NoteOff { note, velocity: _ } => {
                    self.off(*note);
                }
                ChannelVoiceMsg::PitchBend { bend } => {
                    self.bend(*bend);
                }
                ChannelVoiceMsg::ProgramChange { program } => {
                    self.change_patch(*program as usize);
                }
                ChannelVoiceMsg::ControlChange {
                    control: CC { control, value },
                } => {
                    //eprintln!("Control change from {:?} to {:?}", control, value);
                    let norm = *value as f32 / 127.0;
                    if let Some(&(group, idx)) = self.cc_to_knob.get(control) {
                        match group {
                            KnobGroup::Sound => {
                                self.sound_cc_vals[idx] = norm;
                                for state in self.states.iter_mut() {
                                    state.sound_cc_vals[idx].set_value(norm);
                                }
                            }
                            KnobGroup::Effect => {
                                self.fx_cc_vals[idx] = norm;
                                for state in self.states.iter_mut() {
                                    state.fx_cc_vals[idx].set_value(norm);
                                }
                            }
                        }
                        // Print labels
                        // if let Some(prog) =
                        //     self.patch_table.clone().entries.get(self.current_patch_num)
                        // {
                        //     for lbl in prog
                        //         .effects
                        //         .knob_labels
                        //         .iter()
                        //         .chain(prog.sound_factory.knob_labels.iter())
                        //     {
                        //         if lbl.group == group && lbl.index == idx + 1 {
                        //             eprintln!("{}: {}", lbl.label.to_ascii_uppercase(), value);
                        //         }
                        //     }
                        // }
                    }
                }
                _ => {}
            },
            MidiMsg::ChannelMode { channel: _, msg } => match msg {
                ChannelModeMsg::AllNotesOff => self.release_all(),
                ChannelModeMsg::AllSoundOff => self.all_sounds_off(),
                _ => {}
            },
            MidiMsg::SystemRealTime { msg } => {
                if msg == &SystemRealTimeMsg::SystemReset {
                    return Some(RelayedMessage::SystemReset);
                }
            }
            _ => {}
        }
        None
    }

    fn find_next_state(&mut self) -> usize {
        for i in self.next.iter() {
            if self.recent_pitches[i.a()].is_none() {
                //println!("adding new voice!");
                return self.claim_state(i);
            }
        }
        self.next = match self.config.voice_stealing {
            VoiceStealingConfig::LegatoOldest => self.next,
            VoiceStealingConfig::LegatoLast => ModNumC::new(self.next.a() + (N - 1)),
        };
        self.pitch2state[self.recent_pitches[self.next.a()].unwrap() as usize] = None;
        self.release(self.next.a());
        //println!("Recent pitches state after steal: {:?}", self.recent_pitches);
        self.claim_state(self.next)
    }

    fn claim_state(&mut self, state: ModNumC<usize, N>) -> usize {
        let next = state.a();
        self.next = state + 1;
        next
    }

    fn on(&mut self, pitch: u8, velocity: u8) {
        self.master_volume.set_value(0.2);
        let selected = self.find_next_state();
        self.states[selected].note_on(pitch, velocity);
        self.pitch2state[pitch as usize] = Some(selected);
        self.recent_pitches[selected] = Some(pitch);
        //println!("recent pitches: {:?}", self.recent_pitches);
    }

    fn off(&mut self, pitch: u8) {
        if let Some(i) = self.pitch2state[pitch as usize] {
            if self.recent_pitches[i] == Some(pitch) {
                self.release(i);
            }
            self.pitch2state[pitch as usize] = None;
        }
    }

    pub fn change_patch_button(&mut self, button: PatchButton) {
        let offset = match button {
            PatchButton::Right => 1,
            PatchButton::Left => -1,
        };

        self.change_patch_with_offset(offset)
    }
    fn change_patch_with_offset(&mut self, offset: i32) {
        let len = self.patch_table.entries.len();
        if len == 0 {
            return; // No patches, nothing to do
        }
        // Use modulo arithmetic for wrap-around
        let new_num = (self.current_patch_num as i32 + offset).rem_euclid(len as i32);
        self.current_patch_num = new_num as usize;
    }
    fn apply_init_cc_vals(&mut self) {
        let table = self.patch_table.clone();
        if let Some(entry) = table.entries.get(self.current_patch_num) {
            // 1. Apply effect initial CCs to effect knobs
            for (i, &val) in entry.effects.initial_cc.iter().enumerate() {
                println!("FX {}, {}", i, val);
                if i < self.fx_cc_vals.len() {
                    self.fx_cc_vals[i] = val;
                    for state in self.states.iter_mut() {
                        if i < state.effect_cc_count {
                            state.fx_cc_vals[i].set_value(val);
                        }
                    }
                }
            }
        }

        // 2. Apply sound initial CCs to sound parameters
        for (i, &val) in self.patch_table.entries[self.current_patch_num]
            .sound_factory
            .initial_cc
            .iter()
            .enumerate()
        {
            // Ensure we don't go out of bounds for your sound CC array
            if i < self.sound_cc_vals.len() {
                self.sound_cc_vals[i] = val;
                // If your states also store sound CCs, update them similarly:
                for state in self.states.iter_mut() {
                    if i < state.sound_cc_count {
                        state.sound_cc_vals[i].set_value(val);
                    }
                }
            }
        }
    }
    fn change_patch(&mut self, program: usize) {
        let table = self.patch_table.clone();
        if let Some(entry) = table.entries.get(program) {
            self.synth_func = entry.sound_factory.build();
            let tuner = entry.tuning.clone();
            self.set_midi_to_hz(tuner);
            self.current_patch_num = program;
            let new_sound_net = self.sound();
            let new_fx_net = entry.effects.clone().build(&self.states[0]);
            self.mix_net // todo: make fade time for effects configurable?
                .crossfade(self.fx_node_id, Fade::Smooth, 0.5, Box::new(new_fx_net));
            self.mix_net.crossfade(
                self.sound_node_id,
                Fade::Smooth,
                0.01,
                Box::new(new_sound_net),
            );
            self.mix_net.commit();
            self.apply_init_cc_vals();
        }
    }

    fn bend(&mut self, bend: u16) {
        for state in self.states.iter_mut() {
            state.bend(bend);
        }
    }

    fn sound_at(&self, i: usize) -> Box<dyn AudioUnit> {
        (self.synth_func)(&self.states[i])
    }

    fn release(&mut self, i: usize) {
        self.recent_pitches[i] = None;
        self.states[i].note_off();
    }

    fn release_all(&mut self) {
        for i in 0..N {
            self.release(i);
        }
    }

    fn all_sounds_off(&mut self) {
        self.master_volume.set_value(0.0);
    }
}
