use crate::ios::midi::PatchButtonEvent::Ignore;
use crate::note_velocity_from;
use circular_buffer::CircularBuffer;
use midi_msg::{Channel, ChannelModeMsg, ChannelVoiceMsg, MidiMsg, SystemRealTimeMsg};

#[derive(Clone)]
pub struct ButtonEventProcessor {
    event_tracker: CircularBuffer<3, [u8; 2]>,
    left_right_array: Option<[u8; 2]>,
    restart: Option<u8>,
    shutdown: Option<u8>,
}

pub enum PatchButtonEvent {
    ChangeProgram(i32),
    Save,
    Restart,
    Shutdown,
    Ignore,
}

impl ButtonEventProcessor {
    pub fn new(
        left_right_array: Option<[u8; 2]>,
        restart: Option<u8>,
        shutdown: Option<u8>,
    ) -> Self {
        ButtonEventProcessor {
            event_tracker: Default::default(),
            left_right_array,
            restart,
            shutdown,
        }
    }

    pub fn process_event(&mut self, cc_idx: &u8, control: &u8) -> PatchButtonEvent {
        if let Some(l_r) = self.left_right_array {
            if let Some(idx) = l_r.iter().position(|&x| x == *cc_idx) {
                let mut latest = self.event_tracker.back().unwrap().clone();
                latest[idx] = control.clone();
                self.push_event(latest);
                return self.decode_button_event();
            };
            return Ignore;
        }
        Ignore
    }
    fn decode_button_event(&self) -> PatchButtonEvent {
        if self.event_tracker.is_full() {
            let tail = self.event_tracker.get(0).unwrap();
            let middle = self.event_tracker.get(1).unwrap();
            let head = self.event_tracker.get(2).unwrap();
            match (tail, middle, head) {
                (&[0, 0], &[0, 127], &[0, 0]) => PatchButtonEvent::ChangeProgram(1), // right
                (&[0, 0], &[127, 0], &[0, 0]) => PatchButtonEvent::ChangeProgram(-1), // left
                (&[0, 0], _, &[127, 127]) => PatchButtonEvent::Save,
                _ => PatchButtonEvent::Ignore,
            }
        } else {
            Ignore
        }
    }

    fn push_event(&mut self, left_right: [u8; 2]) {
        self.event_tracker.push_back(left_right);
    }
}

impl SynthMsg {
    /// Returns MIDI `All Notes Off` message. This releases all current sounds.
    pub fn all_notes_off() -> Self {
        Self::mode_msg(ChannelModeMsg::AllNotesOff)
    }

    /// Returns MIDI `All Sound Off` message. This shuts off all current sounds immediately.
    pub fn all_sound_off() -> Self {
        Self::mode_msg(ChannelModeMsg::AllSoundOff)
    }

    fn mode_msg(msg: ChannelModeMsg) -> Self {
        Self {
            msg: MidiMsg::ChannelMode {
                channel: Channel::Ch1,
                msg,
            },
        }
    }

    /// Returns MIDI `System Reset` message.
    pub fn system_reset() -> Self {
        Self::system_real_time_msg(SystemRealTimeMsg::SystemReset)
    }

    fn system_real_time_msg(msg: SystemRealTimeMsg) -> Self {
        Self {
            msg: MidiMsg::SystemRealTime { msg },
        }
    }

    /// Returns MIDI `Program Change` message. This selects the synthesizer sound with the given index.
    pub fn patch_change(program: u8) -> Self {
        Self {
            msg: MidiMsg::ChannelVoice {
                channel: Channel::Ch1,
                msg: ChannelVoiceMsg::ProgramChange { program },
            },
        }
    }

    /// Returns MIDI note and velocity information if pertinent
    pub fn note_velocity(&self) -> Option<(u8, u8)> {
        note_velocity_from(&self.msg)
    }
}

#[derive(Clone, Debug)]
/// Packages a [`MidiMsg`](https://crates.io/crates/midi-msg) with a designated `Speaker` to output the sound
/// corresponding to the message.
pub struct SynthMsg {
    pub msg: MidiMsg,
}
