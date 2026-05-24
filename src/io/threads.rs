use crate::config_builder::GlobalConfig;
use crate::io::midi::SynthMsg;
use crate::io::synth::{Speaker, Synth};
use crate::patch_builder::PatchTable;
use crossbeam_queue::SegQueue;
use crossbeam_utils::atomic::AtomicCell;
use midi_msg::{MidiMsg, SystemRealTimeMsg};
use midir::{MidiInput, MidiInputPort};
use std::sync::Arc;

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
        let mut player = crate::io::synth::SynthPlayer::<N>::new(patch_table, cnf);
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
    inner_start_output_thread(
        midi_msgs,
        crate::io::synth::SynthPlayer::<N>::new(patch_table, cnf),
    );
}

fn inner_start_output_thread<const N: usize>(
    midi_msgs: Arc<SegQueue<MidiMsg>>,
    mut player: crate::io::synth::SynthPlayer<N>,
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
