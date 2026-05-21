use crate::config_builder::{FreeVoiceStrategy, GlobalConfig, VoiceStealingConfig};
use crate::effects::master_limiter;
use crate::effects_builders::FxChainFactory;
use crate::patch_builder::KnobGroup;
use crate::{
    NUM_MIDI_VALUES, SharedMidiState, SynthFunc, note_velocity_from, patch_builder::PatchTable,
};
use anyhow::{anyhow, bail};
use bare_metal_modulo::*;
use cpal::{
    Device, FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig,
    SupportedBufferSize,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use crossbeam_queue::SegQueue;
use crossbeam_utils::atomic::AtomicCell;
use fastrand::u8;
use fundsp::prelude::U2;
use fundsp::prelude64::split;
use fundsp::{
    net::Net,
    prelude::AudioUnit,
    prelude64::{shared, var},
    shared::Shared,
};
use midi_msg::ControlChange::CC;
use midi_msg::{Channel, ChannelModeMsg, ChannelVoiceMsg, MidiMsg, SystemRealTimeMsg};
use midir::{Ignore, MidiInput, MidiInputPort};
use read_input::{InputBuild, shortcut::input};
use std::collections::HashMap;
use std::sync::Arc;

enum PatchButton {
    Right,
    Left,
}

#[derive(Clone, Debug)]
/// Packages a [`MidiMsg`](https://crates.io/crates/midi-msg) with a designated `Speaker` to output the sound
/// corresponding to the message.
pub struct SynthMsg {
    pub msg: MidiMsg,
    pub speaker: Speaker,
}

impl SynthMsg {
    /// Returns MIDI `All Notes Off` message. This releases all current sounds.
    pub fn all_notes_off(speaker: Speaker) -> Self {
        Self::mode_msg(ChannelModeMsg::AllNotesOff, speaker)
    }

    /// Returns MIDI `All Sound Off` message. This shuts off all current sounds immediately.
    pub fn all_sound_off(speaker: Speaker) -> Self {
        Self::mode_msg(ChannelModeMsg::AllSoundOff, speaker)
    }

    fn mode_msg(msg: ChannelModeMsg, speaker: Speaker) -> Self {
        Self {
            msg: MidiMsg::ChannelMode {
                channel: midi_msg::Channel::Ch1,
                msg,
            },
            speaker,
        }
    }

    /// Returns MIDI `System Reset` message.
    pub fn system_reset(speaker: Speaker) -> Self {
        Self::system_real_time_msg(SystemRealTimeMsg::SystemReset, speaker)
    }

    fn system_real_time_msg(msg: SystemRealTimeMsg, speaker: Speaker) -> Self {
        Self {
            msg: MidiMsg::SystemRealTime { msg },
            speaker,
        }
    }

    /// Returns MIDI `Program Change` message. This selects the synthesizer sound with the given index.
    pub fn patch_change(program: u8, speaker: Speaker) -> Self {
        Self {
            msg: MidiMsg::ChannelVoice {
                channel: midi_msg::Channel::Ch1,
                msg: ChannelVoiceMsg::ProgramChange { program },
            },
            speaker,
        }
    }

    /// Returns MIDI note and velocity information if pertinent
    pub fn note_velocity(&self) -> Option<(u8, u8)> {
        note_velocity_from(&self.msg)
    }
}

/// Starts a thread that monitors MIDI input events from the source specified by `in_port`. Each message received is
/// stored in a `SynthMsg` object and placed in the `midi_msgs` queue.
///
/// If `true` is stored in `quit`, the thread exits and it sends a MIDI `SystemReset` message.
/// If `print_incoming_msg` is `true`, each incoming MIDI message will be printed to the console.
///
/// The functions `get_first_midi_device()` and `choose_midi_device()` are examples of how to
/// select a value for `in_port`.
pub fn start_input_thread(
    midi_msgs: Arc<SegQueue<SynthMsg>>,
    midi_in: MidiInput,
    in_port: MidiInputPort,
    quit: Arc<AtomicCell<bool>>,
) {
    start_generic_input_thread(
        |msg| SynthMsg {
            msg,
            speaker: Speaker::Both,
        },
        SynthMsg::system_reset(Speaker::Both),
        midi_msgs,
        midi_in,
        in_port,
        quit,
    )
}

/// Starts a thread that monitors MIDI input events from the source specified by `in_port`. Each `MidiMsg` object
/// received is placed in the `midi_msgs` queue.
///
/// If `true` is stored in `quit`, the thread exits and it sends a MIDI `SystemReset` message.
/// If `print_incoming_msg` is `true`, each incoming MIDI message will be printed to the console.
///
/// The functions `get_first_midi_device()` and `choose_midi_device()` are examples of how to
/// select a value for `in_port`.
pub fn start_midi_input_thread(
    midi_msgs: Arc<SegQueue<MidiMsg>>,
    midi_in: MidiInput,
    in_port: MidiInputPort,
    quit: Arc<AtomicCell<bool>>,
) {
    start_generic_input_thread(
        |msg| msg,
        MidiMsg::SystemRealTime {
            msg: SystemRealTimeMsg::SystemReset,
        },
        midi_msgs,
        midi_in,
        in_port,
        quit,
    )
}

fn start_generic_input_thread<M: Send + 'static, F: Send + 'static + Fn(MidiMsg) -> M>(
    encoder: F,
    reset: M,
    midi_msgs: Arc<SegQueue<M>>,
    midi_in: MidiInput,
    in_port: MidiInputPort,
    quit: Arc<AtomicCell<bool>>,
) {
    std::thread::spawn(move || {
        let _conn_in = midi_in
            .connect(
                &in_port,
                "midir-read-input",
                input_callback(encoder, midi_msgs.clone()),
                (),
            )
            .unwrap();
        while !quit.load() {}
        midi_msgs.push(reset);
        quit.store(false);
    });
}

fn input_callback<M: Send + 'static, F: Send + 'static + Fn(MidiMsg) -> M>(
    encoder: F,
    midi_msgs: Arc<SegQueue<M>>,
) -> impl Fn(u64, &[u8], &mut ()) {
    move |_stamp, message, _| {
        let (msg, _len) = MidiMsg::from_midi(message).unwrap();
        midi_msgs.push(encoder(msg));
    }
}

/// Plays sounds according to instructions received in the `midi_msgs` queue. Synthesizer sounds may be selected with
/// MIDI `Program Change` messages that reference sounds stored in `patch_table`.
///
/// The constant value `N` is the number of distinct sounds it can emit. Each MIDI `Note On` message uses one distinct
/// sound. When a number of `Note On` messages greater than `N` has been received, the sound used by the oldest `Note On`
/// message is reused for the new `Note On` message.
///
/// Setting `N = 1` yields a monophonic synthesizer. Setting `N = 10` should suffice for most purposes.
///
/// If a `SystemReset` MIDI message is received, the thread exits.
pub fn start_output_thread<const N: usize>(
    midi_msgs: Arc<SegQueue<SynthMsg>>,
    patch_table: Arc<PatchTable>,
    config: Option<GlobalConfig>,
) {
    let cnf = config.unwrap_or_default();
    println!("{:?}", cnf);
    std::thread::spawn(move || {
        let mut player = StereoPlayer::<N>::new(patch_table, cnf);
        player.run_output(midi_msgs).unwrap();
    });
}

/// Plays sounds according to `MidiMsg` objects received in the `midi_msgs` queue. Synthesizer sounds may be selected with
/// MIDI `Program Change` messages that reference sounds stored in `patch_table`.
///
/// The constant value `N` is the number of distinct sounds it can emit. Each MIDI `Note On` message uses one distinct
/// sound. When a number of `Note On` messages greater than `N` has been received, the sound used by the oldest `Note On`
/// message is reused for the new `Note On` message.
///
/// Setting `N = 1` yields a monophonic synthesizer. Setting `N = 10` should suffice for most purposes.
///
/// If a `SystemReset` MIDI message is received, the thread exits.
pub fn start_midi_output_thread<const N: usize>(
    midi_msgs: Arc<SegQueue<MidiMsg>>,
    patch_table: Arc<PatchTable>,
    config: Option<GlobalConfig>,
) {
    let cnf = config.unwrap_or_default();
    inner_start_output_thread(midi_msgs, StereoPlayer::<N>::new(patch_table, cnf));
}

fn inner_start_output_thread<const N: usize>(
    midi_msgs: Arc<SegQueue<MidiMsg>>,
    mut player: StereoPlayer<N>,
) {
    let relay_out = Arc::new(SegQueue::new());
    let relay_in = relay_out.clone();
    std::thread::spawn(move || {
        loop {
            if let Some(msg) = midi_msgs.pop() {
                relay_out.push(SynthMsg {
                    msg,
                    speaker: Speaker::Both,
                })
            }
        }
    });

    std::thread::spawn(move || {
        player.run_output(relay_in).unwrap();
    });
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
trait DubleSpeaker<const N: usize> {
    fn new(patch_table: Arc<PatchTable>, config: GlobalConfig) -> Self;

    fn set_midi_to_hz(&mut self, midi_to_hz: fn(f32) -> f32);

    fn sound(&mut self) -> Net;

    fn run_output(&mut self, midi_msgs: Arc<SegQueue<SynthMsg>>) -> anyhow::Result<()> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(anyhow!("failed to find a default output device"))?;
        let default_config = device.default_output_config().expect("No default config");

        // 2. Query the device's supported buffer size range
        let buffer_size_range = default_config.buffer_size();

        // 3. Choose a valid buffer size based on the hardware's report
        let buffer_size = match buffer_size_range {
            // If the device reports a min/max range, pick a value in between
            SupportedBufferSize::Range { min, max } => {
                let target = 1024; // Your desired size
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
        let config = cpal::StreamConfig {
            channels: default_config.channels(),
            sample_rate: default_config.sample_rate(),
            buffer_size: buffer_size,
        };
        println!("Config: {:?}", config);
        match default_config.sample_format() {
            SampleFormat::F32 => self.run_synth::<f32>(midi_msgs, device, config.into()),
            SampleFormat::I16 => self.run_synth::<i16>(midi_msgs, device, config.into()),
            SampleFormat::U16 => self.run_synth::<u16>(midi_msgs, device, config.into()),
            sample_format => panic!("Unsupported sample format '{sample_format}'"),
        }
    }

    fn decode(&mut self, speaker: Speaker, msg: &MidiMsg) -> Option<RelayedMessage>;
    fn run_synth<T: Sample + SizedSample + FromSample<f32>>(
        &mut self,
        midi_msgs: Arc<SegQueue<SynthMsg>>,
        device: Device,
        config: StreamConfig,
    ) -> anyhow::Result<()> {
        Self::warm_up(midi_msgs.clone());
        let mut done = false;
        while !done {
            let stream = self.get_stream::<T>(&config, &device)?;
            stream.play()?;
            if self.handle_messages(midi_msgs.clone()) == RelayedMessage::SystemReset {
                done = true;
            }
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

    fn get_stream<T: Sample + SizedSample + FromSample<f32>>(
        &mut self,
        config: &StreamConfig,
        device: &Device,
    ) -> anyhow::Result<Stream> {
        let sample_rate = config.sample_rate as f64;
        let mut sound = self.sound();
        sound.reset();
        sound.set_sample_rate(sample_rate);
        let mut next_value = move || sound.get_stereo();
        let channels = config.channels as usize;
        let err_fn = |err| eprintln!("Error on stream: {err}");
        device
            .build_output_stream(
                config,
                move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                    write_data(data, channels, &mut next_value)
                },
                err_fn,
                None,
            )
            .or_else(|err| bail!("{err:?}"))
    }
}

/// The default player that has one stereo stream in and one out (U2 inputs, U2 outputs)
struct StereoPlayer<const N: usize> {
    center_source: VoiceManager<N>,
}

impl<const N: usize> DubleSpeaker<N> for StereoPlayer<N> {
    fn new(patch_table: Arc<PatchTable>, config: GlobalConfig) -> Self {
        let center_source = VoiceManager::<N>::new(patch_table.clone(), config);
        Self { center_source }
    }

    fn set_midi_to_hz(&mut self, midi_to_hz: fn(f32) -> f32) {
        self.center_source.set_midi_to_hz(midi_to_hz);
    }

    fn sound(&mut self) -> Net {
        self.center_source.sound()
    }

    fn decode(&mut self, _speaker: Speaker, msg: &MidiMsg) -> Option<RelayedMessage> {
        let result = None;
        result.or(self.center_source.decode(msg))
    }
}

/// Presents a list of items to be selected via console input. Used in multiple
/// [example](https://github.com/gjf2a/nabi_core/tree/master/examples) programs.
pub fn console_choice_from<T, F: Fn(&T) -> &str>(
    prompt: &str,
    choices: &Vec<T>,
    prompt_func: F,
) -> usize {
    for i in 0..choices.len() {
        println!("{}: {}", i + 1, prompt_func(&choices[i]));
    }
    let prompt = format!("{prompt}: ");
    input().msg(prompt).inside(1..=choices.len()).get() - 1
}

/// Returns a handle to the first MIDI device detected.
pub fn get_first_midi_device(midi_in: &mut MidiInput) -> anyhow::Result<MidiInputPort> {
    midi_in.ignore(Ignore::None);
    let in_ports = midi_in.ports();
    if in_ports.is_empty() {
        bail!("No MIDI devices attached")
    } else {
        let device_name = midi_in.port_name(&in_ports[0])?;
        println!("Chose MIDI device {device_name}");
        Ok(in_ports[0].clone())
    }
}

/// Allows selecting a MIDI device via the console from a complete list of MIDI devices.
/// The basic concept can be a model of how to do this in a GUI setting.
pub fn choose_midi_device(midi_in: &mut MidiInput) -> anyhow::Result<MidiInputPort> {
    midi_in.ignore(Ignore::None);
    let in_ports = midi_in.ports();
    match in_ports.len() {
        0 => bail!("No MIDI devices attached"),
        1 => get_first_midi_device(midi_in),
        _ => {
            let mut choices = vec![];
            for port in in_ports.iter() {
                choices.push((midi_in.port_name(port)?, port));
            }
            let c = console_choice_from("Select MIDI Device", &choices, |choice| choice.0.as_str());
            Ok(choices[c].1.clone())
        }
    }
}

fn write_data<T: Sample + FromSample<f32>>(
    output: &mut [T],
    channels: usize,
    next_sample: &mut dyn FnMut() -> (f32, f32),
) {
    for frame in output.chunks_mut(channels) {
        let sample = next_sample();
        let left: T = Sample::from_sample::<f32>(sample.0);
        let right: T = Sample::from_sample::<f32>(sample.1);

        for (channel, sample) in frame.iter_mut().enumerate() {
            *sample = if channel & 1 == 0 { left } else { right };
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum RelayedMessage {
    SynthChange,
    SystemReset,
}

/// Single sound emitter that decodes midi and manages voices - used by StereoPlayer and LRPlayer to manage output.
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
    effects: FxChainFactory,
    master_fx_net: Net,
    current_patch_num: usize,
    sound_cc_vals: Vec<f32>,
    fx_cc_vals: Vec<f32>,
    cc_to_knob: HashMap<u8, (KnobGroup, usize)>, // CC → (group, 0‑based index)
}

impl<const N: usize> VoiceManager<N> {
    fn new(patch_table: Arc<PatchTable>, config: GlobalConfig) -> Self {
        // Build CC → knob mapping
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
        let fx_cc_array = first_table.effects.initial_cc.clone();
        let sound_cc_array = first_table.sound_factory.initial_cc.clone();
        let tuner = first_table.tuning;

        let states = [(); N].map(|_| {
            SharedMidiState::new(
                &config.sound_cc_mapping,
                &config.fx_cc_mapping,
                &sound_cc_array,
                &fx_cc_array,
                tuner,
            )
        });
        let master_fx_net = first_table.effects.clone().build(&states[0].clone());
        let mut s = Self {
            states,
            next: ModNumC::new(0),
            pitch2state: [None; NUM_MIDI_VALUES],
            recent_pitches: [None; N],
            synth_func,
            master_volume: shared(0.15),
            patch_table,
            config: config.clone(),
            effects: first_table.effects.clone(),
            sound_cc_vals: vec![0.0; sound_len],
            fx_cc_vals: vec![0.0; effect_len],
            cc_to_knob,
            current_patch_num: 0,
            master_fx_net,
        };
        s.apply_init_cc_vals();
        s
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
                (sound * vol)
            }
            _ => panic!("Unsupported output count on synth! use either U1 (mono) or U2 (stereo)"),
        };
        mix >> master_limiter() >> self.master_fx_net.clone()
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
                    self.change_patch(program);
                    return Some(RelayedMessage::SynthChange);
                }
                ChannelVoiceMsg::ControlChange {
                    control: CC { control, value },
                } => {
                    eprintln!("Control change from {:?} to {:?}", control, value);
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
        // 1. Apply effect initial CCs to effect knobs
        for (i, &val) in self.effects.initial_cc.iter().enumerate() {
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
    fn change_patch(&mut self, program: &u8) {
        let table = self.patch_table.clone();
        if let Some(entry) = table.entries.get(*program as usize) {
            self.synth_func = entry.sound_factory.build();
            self.effects = entry.effects.clone();
            let tuner = entry.tuning;
            self.set_midi_to_hz(tuner);
            self.effects.build(&self.states[0]);
            self.current_patch_num = *program as usize;
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
