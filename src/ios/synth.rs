use crate::common::params::CcInit;
use crate::config_builder::{
    ConfigurableMappings, FreeVoiceStrategy, GlobalConfig, ProgramsFile, TomlOrderConfig,
    VoiceStealingConfig, build_patch_table,
};
use crate::effects::master_fx::master_limiter;
use crate::ios::display::KeyboardDisplay;
pub use crate::ios::midi::SynthMsg;
use crate::ios::midi::{ButtonEventProcessor, PatchButtonEvent, RelayedMessage};
use crate::patch_builder::{KnobGroup, PatchDef};
use crate::sound_engine::sound_building::SynthFunc;
use crate::{
    NUM_MIDI_VALUES, SharedMidiState, patch_builder::PatchTable, shared_array_to_f32_array,
};
use anyhow::{anyhow, bail};
use bare_metal_modulo::*;
use chrono::Local;
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
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

struct AudioBuffers {
    output: BufferVec,
    input: BufferVec,
}
pub trait Synth<const N: usize> {
    fn new(patch_table: Arc<PatchTable>, config: GlobalConfig) -> Self;

    fn run_output(&mut self, midi_msgs: Arc<SegQueue<SynthMsg>>) -> anyhow::Result<()>;

    fn decode(&mut self, msg: &MidiMsg) -> Option<RelayedMessage>;
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
        }
    }

    fn handle_messages(&mut self, midi_msgs: Arc<SegQueue<SynthMsg>>) -> RelayedMessage {
        loop {
            if let Some(msg) = midi_msgs.pop()
                && let Some(relayed) = self.decode(&msg.msg)
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
    buffers: AudioBuffers,
}
impl<const N: usize> Synth<N> for SynthPlayer<N> {
    fn new(patch_table: Arc<PatchTable>, config: GlobalConfig) -> Self {
        let voice_manager = VoiceManager::<N>::new(patch_table.clone(), config);
        let mut s = Self {
            voice_manager,
            buffers: AudioBuffers {
                output: BufferVec::new(2),
                input: BufferVec::new(2),
            },
        };
        s.voice_manager
            .update_screen(&patch_table.clone().entries[0].toml.name, "")
            .unwrap();
        s
    }

    fn run_output(&mut self, midi_msgs: Arc<SegQueue<SynthMsg>>) -> anyhow::Result<()> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(anyhow!("failed to find a default output device"))?;
        let default_config = device.default_output_config().expect("No default config");

        let buffer_size_range = default_config.buffer_size();

        let buffer_size = match buffer_size_range {
            // If the device reports a min/max range, pick a value in between
            SupportedBufferSize::Range { min, max } => {
                let target = 390; // todo: make it configurable?
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

    fn decode(&mut self, msg: &MidiMsg) -> Option<RelayedMessage> {
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
                *config,
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

/// Single sound emitter that decodes midi and manages voices - used by SynthPlayer and LRPlayer to manage output.
//#[derive(Clone)]
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
    cc_to_logical_num: HashMap<u8, (KnobGroup, usize)>,
    button_event_processor: ButtonEventProcessor,
    keyboard_display: Option<KeyboardDisplay>,
}

impl<const N: usize> VoiceManager<N> {
    fn new(patch_table: Arc<PatchTable>, config: GlobalConfig) -> Self {
        let cc_to_logical_num = Self::get_cc_map(&config);
        let first_table = &patch_table.clone().entries[0];
        let synth_func = first_table.sound_factory.build_synth();
        let fx_cc_array = &first_table.effects.get_initial_cc();
        let sound_cc_array = &first_table.sound_factory.get_initial_cc();
        let tuner = first_table.tuning;
        let mut master_fx_net = Net::new(2, 2);
        println!("sound cc array: {:?}", sound_cc_array);
        println!("fx cc array: {:?}", fx_cc_array);
        let states = [(); N].map(|_| {
            SharedMidiState::new(
                &config.sound_cc_mapping,
                &config.fx_cc_mapping,
                sound_cc_array,
                fx_cc_array,
                tuner,
            )
        });

        let fx_node_id = master_fx_net.chain(Box::new(
            first_table.effects.clone().build_chain(&states[0]),
        ));
        let keyboard_display = KeyboardDisplay::try_new();
        let mut s = Self {
            states,
            next: ModNumC::new(0),
            pitch2state: [None; NUM_MIDI_VALUES],
            recent_pitches: [None; N],
            synth_func,
            master_volume: shared(0.15),
            patch_table,
            config: config.clone(),
            cc_to_logical_num,
            current_patch_num: 0,
            fx_node_id,
            mix_net: Net::new(2, 2),
            sound_node_id: NodeId::new(),
            button_event_processor: ButtonEventProcessor::new(
                Some(config.left_right_buttons),
                None,
                None,
            ),
            keyboard_display,
        };
        s.clear_screen().unwrap();
        s.update_screen("NABI Synth", "").unwrap();
        sleep(Duration::from_millis(200));
        s
    }

    fn update_screen(
        &mut self,
        line1: &str,
        line2: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ref mut d) = self.keyboard_display {
            d.set_text(line1, line2)?;
        }
        Ok(())
    }

    fn clear_screen(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ref mut d) = self.keyboard_display {
            d.clear_screen()?;
        }
        Ok(())
    }

    pub fn get_current_patch(&self) -> &PatchDef {
        &self.patch_table.entries[self.current_patch_num]
    }

    /// Flushes the patch state into a new toml file. Adds a pet name to the patch name and a date string to the file.
    fn patch_state_to_toml(&self) -> ProgramsFile {
        let old_toml = &self.get_current_patch().toml;
        let mut new_toml = old_toml.clone();
        let sound_cc_array = shared_array_to_f32_array(&self.states[0].sound_cc_vals);
        let fx_cc_array = shared_array_to_f32_array(&self.states[0].fx_cc_vals);

        let mut new_params = (*self.get_current_patch()).clone();

        new_params
            .sound_factory
            .params
            .apply_cc_state(&sound_cc_array);
        let new_sound_values = new_params.sound_factory.params.to_toml_values();

        let existing_mapping = old_toml.sound.as_ref().and_then(|s| s.mapping.clone());
        new_toml.sound = Some(ConfigurableMappings {
            values: Some(new_sound_values),
            mapping: existing_mapping,
        });

        if let Some(ref mut toml_effects_section) = new_toml.effects {
            let mut new_fx_map: HashMap<String, ConfigurableMappings> = HashMap::new();
            for def in new_params.effects.definitions.iter() {
                for fx in def.iter() {
                    let mut new_fx = (**fx).clone();
                    new_fx.apply_cc_state(&fx_cc_array);
                    let values = new_fx.to_toml_values();
                    let fx_c_map = ConfigurableMappings {
                        values: Some(values),
                        mapping: old_toml
                            .effects
                            .as_ref()
                            .and_then(|section| section.configs.as_ref())
                            .and_then(|hash_map| hash_map.get(fx.name))
                            .and_then(|config_m| config_m.mapping.clone()),
                    };
                    new_fx_map.insert(fx.name.to_string(), fx_c_map);
                }
            }
            toml_effects_section.configs = Some(new_fx_map);
        }
        let pet_name: Option<String> = petname::petname(1, "");
        new_toml
            .name
            .extend(pet_name.unwrap_or("-x".to_string()).chars());
        let new_vec = vec![new_toml];
        ProgramsFile::new(new_vec)
    }
    pub fn save_patch_state(&mut self) {
        let toml = self.patch_state_to_toml();
        let new_file_name = format!(
            "{}-{}.toml",
            self.get_current_patch().toml.name,
            Local::now().format("%Y-%m-%d_%H-%M-%S.%3f")
        );
        let new_patch_toml_str =
            toml::to_string_pretty(&toml).expect("failed to serialize patch state to TOML");
        let mut patches_path = self.config.patches_path.clone();
        patches_path.push(new_file_name);
        let new_patch = build_patch_table(&toml.program);
        self.current_patch_num += 1;
        Arc::make_mut(&mut self.patch_table)
            .entries
            .insert(self.current_patch_num, new_patch.entries[0].clone());
        let mut new_order_vec: Vec<String> = Vec::with_capacity(self.patch_table.entries.len());
        for entry in &self.patch_table.entries {
            new_order_vec.push(entry.toml.name.clone())
        }
        let toml_order_config = toml::to_string_pretty(&TomlOrderConfig {
            patch_order: new_order_vec,
        })
        .expect("failed to serialize Ordering to string from TOML");
        let mut ordering_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        ordering_path.push("config/order.toml");
        fs::write(ordering_path, toml_order_config).expect("failed to write Order state to TOML");
        fs::write(patches_path, new_patch_toml_str).expect("failed to write patch state to TOML");
    }

    pub fn handle_button_event(&mut self, event: PatchButtonEvent) {
        match event {
            PatchButtonEvent::ChangeProgram(offset) => {
                println!("Changing program by offset {}", offset);
                self.change_patch_with_offset(offset)
            }
            PatchButtonEvent::Save => {
                println!("Saving patch state");
                self.save_patch_state()
            }
            PatchButtonEvent::Restart => todo!("restart synth"),
            PatchButtonEvent::Shutdown => todo!("shutdown synth"),
            PatchButtonEvent::Ignore => {}
        }
    }

    /// Rebuild current sound based on the state of the patch table - without committing.
    fn rebuild_and_replace_sound(&mut self) {
        let new_synth = self.patch_table.entries[self.current_patch_num]
            .sound_factory
            .build_synth();
        self.synth_func = new_synth;
        let new_sound_net = self.sound();
        self.mix_net.crossfade(
            self.sound_node_id,
            Fade::Smooth,
            0.01,
            Box::new(new_sound_net),
        );
    }

    /// Rebuild current fx chain based on the state of the patch table - without committing.
    fn rebuild_and_replace_fx_chain(&mut self) {
        let entry = &self.patch_table.entries[self.current_patch_num].effects;
        let new_fx_net = entry.clone().build_chain(&self.states[0]);
        self.mix_net
            .crossfade(self.fx_node_id, Fade::Smooth, 0.5, Box::new(new_fx_net));
    }

    /// Commit patch Net changes (sound rebuilt, effects chain rebuild, etc.)
    fn commit_patch_changes(&mut self) {
        self.mix_net.commit()
    }

    fn get_cc_map(config: &GlobalConfig) -> HashMap<u8, (KnobGroup, usize)> {
        let mut cc_to_logical_num = HashMap::new();
        for (i, &cc) in config.sound_cc_mapping.iter().enumerate() {
            cc_to_logical_num.insert(cc, (KnobGroup::Sound, i));
        }
        for (i, &cc) in config.fx_cc_mapping.iter().enumerate() {
            cc_to_logical_num.insert(cc, (KnobGroup::Effect, i));
        }
        println!("{:?}", cc_to_logical_num);
        cc_to_logical_num
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
                .chain(Box::new(entry.effects.clone().build_chain(&self.states[0])));
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
                    // quantized to 0.0-1.0 with 0.01 steps:
                    if let Some(&(group, idx)) = self.cc_to_logical_num.get(control) {
                        let norm = *value as f32 / 127.0;
                        let current = self.get_current_patch().clone();
                        let mut cc_line = "".to_string();
                        match group {
                            KnobGroup::Sound => {
                                for state in self.states.iter_mut() {
                                    state.sound_cc_vals[idx].set_value(norm);
                                    if let Some(cc_name) =
                                        current.effects.chain_param_from_cc_index(idx)
                                    {
                                        cc_line =
                                            format!("{} {}", cc_name.name, (norm * 100.0).round());
                                    };
                                }
                            }
                            KnobGroup::Effect => {
                                for state in self.states.iter_mut() {
                                    state.fx_cc_vals[idx].set_value(norm);
                                    if let Some(cc_name) =
                                        current.sound_factory.params.param_from_cc_index(idx)
                                    {
                                        cc_line =
                                            format!("{} {}%", cc_name.name, (norm * 100.0).round())
                                    };
                                }
                            }
                        }
                        self.update_screen(&current.toml.name, &cc_line)
                            .expect("Failed to update screen");
                    } else {
                        let event = self.button_event_processor.process_event(control, value);
                        self.handle_button_event(event);
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
        println!("recent pitches: {:?}", self.recent_pitches);
    }

    fn off(&mut self, pitch: u8) {
        if let Some(i) = self.pitch2state[pitch as usize] {
            if self.recent_pitches[i] == Some(pitch) {
                self.release(i);
            }
            self.pitch2state[pitch as usize] = None;
        }
    }
    fn change_patch_with_offset(&mut self, offset: i32) {
        let len = self.patch_table.entries.len();
        if len == 0 {
            return;
        }
        // Use modulo arithmetic for wrap-around
        let new_num = (self.current_patch_num as i32 + offset).rem_euclid(len as i32);
        self.change_patch(new_num as usize);
    }

    fn apply_init_cc_vals(&mut self) {
        let table = self.patch_table.clone();
        if let Some(entry) = table.entries.get(self.current_patch_num) {
            for (i, &val) in entry.effects.get_initial_cc().iter().enumerate() {
                if i < self.states[0].fx_cc_vals.len() {
                    for state in self.states.iter_mut() {
                        if i < state.effect_cc_count {
                            state.fx_cc_vals[i].set_value(val);
                        }
                    }
                }
            }
        }

        for (i, &val) in self.patch_table.entries[self.current_patch_num]
            .sound_factory
            .get_initial_cc()
            .iter()
            .enumerate()
        {
            if i < self.states[0].sound_cc_vals.len() {
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
            self.synth_func = entry.sound_factory.build_synth();
            let tuner = entry.tuning.clone();
            self.set_midi_to_hz(tuner);
            self.current_patch_num = program;
            self.rebuild_and_replace_fx_chain();
            self.rebuild_and_replace_sound();
            self.commit_patch_changes();
            self.apply_init_cc_vals();
            self.update_screen(&entry.toml.name, "").unwrap();
            //println!("changed to patch: {}", entry.toml.name)
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
