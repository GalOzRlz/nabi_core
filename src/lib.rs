//! This crate enables the construction of synthesizers with live MIDI input and sound synthesis
//! using [fundsp](https://crates.io/crates/fundsp).
//!
//! It is organized as follows:
//! * The crate root contains functions and data structures useful for constucting [fundsp](https://crates.io/crates/fundsp)
//!   sounds.
//!   * [MIDI input messages](https://www.midi.org/specifications-old/item/table-1-summary-of-midi-message) are
//!   converted into `SharedMidiState` objects that translate the sounds represented by those messages into
//!   [fundsp `Shared` atomic variables](https://docs.rs/fundsp/0.10.0/fundsp/audionode/struct.Shared.html).
//!   * `SynthFunc` functions translate `SharedMidiState` objects into specific [fundsp](https://crates.io/crates/fundsp) audio graphs.
//! * The `io` module contains functions and data types for obtaining messages from MIDI devices and playing  
//!   [fundsp](https://crates.io/crates/fundsp) audio graphs through the computer's speakers.
//! * The `sound_builders` module contains functions that wrap [fundsp](https://crates.io/crates/fundsp) audio graphs
//!   into `SynthFunc` functions with a variety of properties.
//! * The `sounds` module contains `SynthFunc` functions that produce a variety of live sounds.

mod backend;
pub mod community_patches;
pub mod config_builder;
mod effects;
mod effects_builders;
mod eqs;
mod factories;
mod instruments;
pub mod io;
mod modulators;
mod oximedia_effects;
pub mod patch_builder;
mod patch_helpers;
pub mod sounds;
pub mod tunings;

use crate::config_builder::MAX_KNOBS_PER_GROUP;
use std::collections::HashMap;
use std::fmt::Debug;
use std::ops::Index;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::patch_builder::{KnobGroup, KnobLabel, SoundBuilder, SoundEntry};
use crate::patch_helpers::Adsr;
use crate::tunings::TunerBuilder;
use fundsp::math::midi_hz;
use fundsp::net::Net;
use fundsp::prelude::{An, AudioUnit, FrameMul};
use fundsp::prelude64::{adsr_live, shared, var};
use fundsp::shared::{Shared, Var};
use midi_msg::MidiMsg;
use serde::de::DeserializeOwned;
use toml::Table;

/// MIDI values for pitch and velocity range from 0 to 127.
pub const MAX_MIDI_VALUE: u8 = 127;

/// Total quantity of distinct MIDI values.
pub const NUM_MIDI_VALUES: usize = MAX_MIDI_VALUE as usize + 1;

/// Control value in response to `Note On` event.
pub const CONTROL_ON: f32 = 1.0;

/// Control value in response to `Note Off` event.
pub const CONTROL_OFF: f32 = -1.0;

/// `SynthFunc` objects translate `SharedMidiState` values into [fundsp](https://crates.io/crates/fundsp) audio graphs.
pub type SynthFunc = Arc<dyn Fn(&SharedMidiState) -> Box<dyn AudioUnit> + Send + Sync>;

pub trait FromTable: Sized {
    fn from_table(table: &Table) -> Self;
}

impl<T> FromTable for T
where
    T: DeserializeOwned + Default,
{
    fn from_table(table: &Table) -> Self {
        let value: toml::Value = table.clone().into();
        T::deserialize(value).unwrap_or_default()
    }
}

#[derive(Clone)]
pub struct SynthFactory {
    pub builder: SoundBuilder,
    pub knob_labels: Vec<KnobLabel>,
    pub config: Table,
    pub initial_cc: Vec<f32>,
}

impl SynthFactory {
    pub fn new(builder_func_name: &str, config: Table, sound_cc_count: usize) -> Self {
        let registry: HashMap<&str, &SoundEntry> = inventory::iter::<SoundEntry>()
            .map(|e| (e.name, e))
            .collect();
        let entry = registry
            .get(builder_func_name)
            .expect("synth with stated function name doesn't exist!");
        let builder = entry.builder.to_owned();
        let mut knob_labels = Vec::new();
        let mut knob_map = HashMap::new();
        for (param_name, default_knob) in entry.cc_params.iter() {
            let mut knob = *default_knob;

            // Clamp or should we ignore?
            if knob < 1 {
                knob = 1;
            }
            if knob > sound_cc_count {
                knob = sound_cc_count;
            }

            knob_map.insert(param_name.to_string(), knob);

            knob_labels.push(KnobLabel {
                group: KnobGroup::Sound,
                index: knob,
                label: format!("{}: {}", param_name, param_name),
            })
        }
        let mut lables = knob_labels.clone();
        let mut initial_knobs = vec![0.0_f32; sound_cc_count.max(1)];
        for v in config.iter() {
            let c_label = v.0;
            for l in lables.iter_mut() {
                if l.label == *c_label {
                    // cc is 1-...
                    initial_knobs
                        .insert(l.index - 1, v.1.as_float().expect("illegal value!") as f32)
                }
            }
        }
        Self {
            builder,
            knob_labels,
            config: config.clone(),
            initial_cc: initial_knobs,
        }
    }

    pub fn build(&self) -> SynthFunc {
        let function = self.builder.clone();
        let config = self.config.clone();
        Arc::new(move |state: &SharedMidiState| -> Box<dyn AudioUnit> { function(state, &config) })
    }
}

/// `SharedMidiState` objects represent as [fundsp `Shared` atomic variables](https://docs.rs/fundsp/latest/fundsp/shared/struct.Shared.html)
/// the following MIDI events:
/// * `Note On`
/// * `Note Off`
/// * `Pitch Bend`
#[derive(Clone)]
pub struct SharedMidiState {
    pitch: Shared,
    velocity: Shared,
    control: Shared,
    pitch_bend: Shared,
    midi_to_hz: fn(f32) -> f32,
    sound_cc_vals: [Shared; MAX_KNOBS_PER_GROUP],
    fx_cc_vals: [Shared; MAX_KNOBS_PER_GROUP],
    sound_cc_count: usize,
    effect_cc_count: usize,
    adsr: Adsr,
}

impl Default for SharedMidiState {
    fn default() -> Self {
        Self {
            pitch: Default::default(),
            velocity: Default::default(),
            control: shared(CONTROL_OFF),
            pitch_bend: shared(1.0),
            midi_to_hz: midi_hz,
            sound_cc_vals: core::array::from_fn(|_| Shared::new(0.0)),
            fx_cc_vals: core::array::from_fn(|_| Shared::new(0.0)),
            sound_cc_count: 0,
            effect_cc_count: 0,
            adsr: Default::default(),
        }
    }
}

impl Debug for SharedMidiState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedMidiState")
            .field("pitch", &self.pitch.value())
            .field("velocity", &self.velocity.value())
            .field("control", &self.control.value())
            .field("pitch_bend", &self.pitch_bend.value())
            .finish()
    }
}

impl SharedMidiState {
    pub fn new(
        sound_cc_mapping: &[u8],
        fx_cc_mapping: &[u8],
        sound_init: &[f32],
        effect_init: &[f32],
        tuner: TunerBuilder,
    ) -> Self {
        println!("sound init: {:?}", sound_init);
        let mut s = Self::default();
        s.sound_cc_count = sound_cc_mapping.len().min(MAX_KNOBS_PER_GROUP);
        s.effect_cc_count = fx_cc_mapping.len().min(MAX_KNOBS_PER_GROUP);
        for i in 0..s.sound_cc_count {
            let val = sound_init.get(i).copied().unwrap_or(0.0);
            s.sound_cc_vals[i].set_value(val);
        }
        for i in 0..s.effect_cc_count {
            let val = effect_init.get(i).copied().unwrap_or(0.0);
            s.fx_cc_vals[i].set_value(val);
        }
        s.set_midi_to_hz(tuner);
        s
    }

    /// Returns n ADSR filter in a `Box`.
    pub fn boxed_adsr(&self) -> Box<dyn AudioUnit> {
        let control = self.control_var();
        Box::new(
            control
                >> adsr_live(
                    self.adsr.attack.value(),
                    self.adsr.decay.value(),
                    self.adsr.sustain.value(),
                    self.adsr.release.value(),
                ),
        )
    }
    fn sound_cc(&self, idx: usize) -> Option<An<Var>> {
        if idx < 1 || idx > self.sound_cc_count {
            return None;
        }
        Some(var(&self.sound_cc_vals[idx - 1]))
    }
    fn fx_cc(&self, idx: usize) -> Option<An<Var>> {
        if idx < 1 || idx > self.effect_cc_count {
            return None;
        }
        Some(var(&self.fx_cc_vals[idx - 1]))
    }

    pub fn get_fx_cc_or(&self, cc: usize, default: f32) -> An<Var> {
        self.fx_cc(cc).unwrap_or(var(&shared(default)))
    }

    pub fn get_sound_cc_or(&self, cc: usize, default: f32) -> An<Var> {
        self.sound_cc(cc).unwrap_or(var(&shared(default)))
    }

    /// Changes how MIDI notes are converted to pitches. Defaults to equal temperament.
    pub fn set_midi_to_hz(&mut self, midi_to_hz: fn(f32) -> f32) {
        self.midi_to_hz = midi_to_hz;
    }

    /// Returns the most recent `Note On` pitch, modified by the most recent `Pitch Bend` event.
    pub fn bent_pitch(&self) -> Net {
        Net::wrap(Box::new(var(&self.pitch_bend) * var(&self.pitch)))
    }

    /// Returns `CONTROL_ON` if `Note On` is the most recent event for this pitch, and `CONTROL_OFF` otherwise.
    pub fn control_var(&self) -> An<Var> {
        var(&self.control)
    }

    /// Returns the current volume.
    ///
    /// The volume is determined from the velocity of the most recent `Note On` event in combination with the
    /// output from the `adjuster`. The `adjuster` should use `control_var()` to determine whether the most recent
    /// event is `Note On` or `Note Off`, and adjust the volume acontrol_changeordingly, whether it is a sudden cutoff or
    /// a gradual release.
    pub fn volume(&self, adjuster: Box<dyn AudioUnit>) -> Net {
        Net::binary(
            Net::wrap(Box::new(var(&self.velocity))),
            Net::wrap(adjuster),
            FrameMul::new(),
        )
    }

    /// Pipes the current `bent_pitch()` into `synth`, then multiplies by `volume(adjuster)` to
    /// produce the final sound.
    pub fn assemble_unpitched_sound(
        &self,
        synth: Box<dyn AudioUnit>,
        adjuster: Box<dyn AudioUnit>,
    ) -> Box<dyn AudioUnit> {
        self.assemble_pitched_sound(
            Box::new(Net::pipe(self.bent_pitch(), Net::wrap(synth))),
            adjuster,
        )
    }

    /// Assumes that the current `bent_pitch()` value has already been incorporated into `pitched_sound`.
    /// It then multiplies by `volume(adjuster)` to produce the final sound.
    pub fn assemble_pitched_sound(
        &self,
        pitched_sound: Box<dyn AudioUnit>,
        adjuster: Box<dyn AudioUnit>,
    ) -> Box<dyn AudioUnit> {
        Box::new(Net::binary(
            Net::wrap(pitched_sound),
            self.volume(adjuster),
            FrameMul::new(),
        ))
    }

    /// Encodes a MIDI `Note On` event as a positive gate signal
    pub fn note_on(&self, pitch: u8, velocity: u8) {
        self.pitch.set_value((self.midi_to_hz)(pitch as f32));
        self.velocity
            .set_value(velocity as f32 / MAX_MIDI_VALUE as f32);
        self.control.set_value(CONTROL_ON);
    }

    /// Encodes a MIDI `Note Off` event.
    pub fn note_off(&self) {
        self.control.set_value(CONTROL_OFF);
    }

    /// Encodes a MIDI `Pitch Bend` event.
    ///
    /// Converts MIDI pitch-bend message to +/- 1 semitone using [this algorithm](https://sites.uci.edu/camp2014/2014/04/30/managing-midi-pitchbend-messages/).
    pub fn bend(&self, bend: u16) {
        self.pitch_bend.set_value(pitch_bend_factor(bend));
    }
}

/// If a given `MidiMsg` object is a `NoteOn` or `NoteOff` message, it returns
/// the note and velocity values of that message.
pub fn note_velocity_from(msg: &MidiMsg) -> Option<(u8, u8)> {
    if let MidiMsg::ChannelVoice { channel: _, msg } = msg {
        match msg {
            midi_msg::ChannelVoiceMsg::NoteOn { note, velocity }
            | midi_msg::ChannelVoiceMsg::NoteOff { note, velocity } => Some((*note, *velocity)),
            _ => None,
        }
    } else {
        None
    }
}

/// Converts MIDI pitch-bend message to frequency multiplier over +/- 1 semitone using [this algorithm](https://sites.uci.edu/camp2014/2014/04/30/managing-midi-pitchbend-messages/).
pub fn pitch_bend_factor(bend: u16) -> f32 {
    2.0_f32.powf(semitone_from(bend) / 12.0)
}

/// Converts MIDI pitch-bend message to +/- 1 semitone using [this algorithm](https://sites.uci.edu/camp2014/2014/04/30/managing-midi-pitchbend-messages/).
pub fn semitone_from(bend: u16) -> f32 {
    (bend as f32 - 8192.0) / 8192.0
}

#[derive(Debug)]
/// When designing sounds, it can be useful to understand their typical output levels. `SoundTestResult` objects
/// track the minimum, maximum, and mean output levels.
pub struct SoundTestResult {
    total: f32,
    count: usize,
    min: f32,
    max: f32,
}

impl SoundTestResult {
    /// Add a new value to this `SoundTestResult`.
    pub fn add_value(&mut self, value: f32) {
        self.count += 1;
        self.total += value;
        if value < self.min {
            self.min = value;
        }
        if value > self.max {
            self.max = value;
        }
    }

    /// Report the mean, minimum, and maximum.
    pub fn report(&self) {
        println!(
            "{} ({}..{})",
            self.total / self.count as f32,
            self.min,
            self.max
        );
    }
}

impl Default for SoundTestResult {
    fn default() -> Self {
        Self {
            total: Default::default(),
            count: Default::default(),
            min: f32::MAX,
            max: f32::MIN,
        }
    }
}

/// Sample rate of 44.1 kHz for use in `SoundTestResult`.
pub const SAMPLE_RATE: f64 = 44100.0;

/// Duration of test for `SoundTestResult`.
pub const DURATION: f64 = 5.0;

const SLEEP_TIME: f64 = 1.0 / SAMPLE_RATE;

impl SoundTestResult {
    /// Tests the given `sound` by playing a middle C note for `DURATION` seconds at `SAMPLE_RATE`.
    /// Returns a `SoundTestResult` that summarizes the resuts.
    pub fn test(sound: Arc<dyn Fn(&SharedMidiState) -> Box<dyn AudioUnit> + Send + Sync>) -> Self {
        let mut result = Self::default();
        let state = SharedMidiState::default();
        let mut sound = sound(&state);
        sound.reset();
        sound.set_sample_rate(SAMPLE_RATE);
        let mut next_value = move || sound.get_mono();
        let start = Instant::now();
        state.note_on(60, 127);
        while start.elapsed().as_secs_f64() < DURATION {
            result.add_value(next_value());
            std::thread::sleep(Duration::from_secs_f64(SLEEP_TIME));
        }
        result
    }
}
