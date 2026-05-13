use std::sync::{Arc, Mutex};

use crossbeam_queue::SegQueue;
use crossbeam_utils::atomic::AtomicCell;
use nabi_core::{
    io::{
        Speaker, SynthMsg, choose_midi_device, console_choice_from, start_input_thread,
        start_output_thread,
    },
    sound_builders::PatchTable,
};
use midir::MidiInput;
use nabi_core::config_builder::create_ordered_patch_table;

fn main() -> anyhow::Result<()> {
    let reset = Arc::new(AtomicCell::new(false));
    let mut quit = false;
    while !quit {
        let mut midi_in = MidiInput::new("midir reading input")?;
        let in_port = choose_midi_device(&mut midi_in)?;
        let midi_msgs = Arc::new(SegQueue::new());
        while reset.load() {}
        start_input_thread(midi_msgs.clone(), midi_in, in_port, reset.clone());
        let patch_table = Arc::new(Mutex::new(create_ordered_patch_table()));
        start_output_thread::<10>(midi_msgs.clone(), patch_table.clone(), None);
        run_chooser(midi_msgs, patch_table.clone(), reset.clone(), &mut quit);
    }
    Ok(())
}

fn run_chooser(
    midi_msgs: Arc<SegQueue<SynthMsg>>,
    patch_table: Arc<Mutex<PatchTable>>,
    reset: Arc<AtomicCell<bool>>,
    quit: &mut bool,
) {
    let main_menu = vec!["Pick New Synthesizer Sound", "Pick New MIDI Device", "Quit"];
    while !*quit && !reset.load() {
        println!("Play notes at will. When ready for a change, select one of the following:");
        match console_choice_from("Choice", &main_menu, |s| *s) {
            0 => {
                let program = {
                    let patch_table = patch_table.lock().unwrap();
                    console_choice_from("Change synth to", &patch_table.entries, |opt| {
                        opt.0.as_str()
                    })
                };
                midi_msgs.push(SynthMsg::program_change(program as u8, Speaker::Both));
            }
            1 => reset.store(true),
            2 => *quit = true,
            _ => panic!("This should never happen."),
        }
    }
}
