use crate::config_builder::GlobalConfig;
use crate::ios::midi::SynthMsg;
use crate::ios::synth::{Synth, SynthPlayer};
use crate::patch_builder::PatchTable;
use crate::tui::get_first_midi_device;
use crossbeam_queue::SegQueue;
use crossbeam_utils::atomic::AtomicCell;
use midi_msg::ControlChange::CC;
use midi_msg::{ChannelVoiceMsg, MidiMsg, SystemRealTimeMsg};
use midir::{MidiInput, MidiInputPort};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::thread::{JoinHandle, sleep};
use std::time::Duration;

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
        |msg| SynthMsg { msg },
        SynthMsg::system_reset(),
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
                "nabi-input",
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
    std::thread::spawn(move || {
        let mut player = SynthPlayer::<N>::new(patch_table, cnf);
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
    inner_start_output_thread(midi_msgs, SynthPlayer::<N>::new(patch_table, cnf));
}

fn inner_start_output_thread<const N: usize>(
    midi_msgs: Arc<SegQueue<MidiMsg>>,
    mut player: SynthPlayer<N>,
) {
    let relay_out = Arc::new(SegQueue::new());
    let relay_in = relay_out.clone();
    std::thread::spawn(move || {
        loop {
            if let Some(msg) = midi_msgs.pop() {
                relay_out.push(SynthMsg { msg })
            }
        }
    });

    std::thread::spawn(move || {
        player.run_output(relay_in).unwrap();
    });
}

#[cfg(target_os = "linux")]
pub fn set_realtime_priority() {
    unsafe {
        let param = libc::sched_param { sched_priority: 50 };
        let ret = libc::sched_setscheduler(0, libc::SCHED_FIFO, &param);
        if ret != 0 {
            eprintln!(
                "Warning: failed to set real-time priority: {} (are you root or have CAP_SYS_NICE?)",
                std::io::Error::last_os_error()
            );
        }
    }
}

#[cfg(not(target_os = "linux"))]
pub fn set_realtime_priority() {}

pub fn get_first_input_port(midi_in: &mut MidiInput) -> MidiInputPort {
    let in_port = loop {
        match get_first_midi_device(midi_in) {
            Ok(port) => break port,
            Err(_) => {
                println!("waiting for midi input device..");
                sleep(Duration::from_millis(1200));
            }
        }
    };
    in_port
}

pub fn cc_mapper_handler(
    midi_msgs: Arc<SegQueue<SynthMsg>>,
    data_tx: Sender<Option<u8>>,
    stop_rx: Receiver<Option<()>>,
) -> JoinHandle<()> {
    let mut cc_val: Option<u8> = None;
    let handler = thread::spawn(move || {
        loop {
            if let Ok(_) = stop_rx.try_recv() {
                let _ = data_tx.send(cc_val);
                return;
            }
            if let Some(midi_msg) = midi_msgs.pop() {
                match midi_msg.msg {
                    MidiMsg::ChannelVoice { channel: _, msg } => match msg {
                        ChannelVoiceMsg::ControlChange {
                            control:
                                CC {
                                    control,
                                    value: _value,
                                },
                        } => cc_val = Some(control),
                        _ => {}
                    },
                    _ => {}
                }
            }
            sleep(Duration::from_millis(10));
        }
    });
    handler
}
