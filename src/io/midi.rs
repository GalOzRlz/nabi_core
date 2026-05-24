use crate::io::synth::Speaker;
use crate::note_velocity_from;
use midi_msg::{Channel, ChannelModeMsg, ChannelVoiceMsg, MidiMsg, SystemRealTimeMsg};

pub enum PatchButton {
    Right,
    Left,
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
                channel: Channel::Ch1,
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
                channel: Channel::Ch1,
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

#[derive(Clone, Debug)]
/// Packages a [`MidiMsg`](https://crates.io/crates/midi-msg) with a designated `Speaker` to output the sound
/// corresponding to the message.
pub struct SynthMsg {
    pub msg: MidiMsg,
    pub speaker: Speaker,
}
