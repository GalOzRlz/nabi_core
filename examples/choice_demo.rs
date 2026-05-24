use crossbeam_queue::SegQueue;
use crossbeam_utils::atomic::AtomicCell;
use midir::MidiInput;
use nabi_core::config_builder::{create_ordered_patch_table, load_global_config};
use nabi_core::io::synth::{Speaker, SynthMsg};
use nabi_core::io::threads::{start_input_thread, start_output_thread};
use nabi_core::patch_builder::PatchTable;
use nabi_core::tui::{console_choice_from, get_first_midi_device};
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    let reset = Arc::new(AtomicCell::new(false));
    let mut quit = false;
    while !quit {
        let global_config = load_global_config("config/global.toml");
        let mut midi_in = MidiInput::new("midir reading input")?;
        let in_port = loop {
            match get_first_midi_device(&mut midi_in) {
                Ok(port) => break port, // exit loop, returning `port`
                Err(_) => {
                    println!("waiting for midi input device..");
                    sleep(Duration::from_millis(1200));
                }
            }
        };

        let midi_msgs = Arc::new(SegQueue::new());
        while reset.load() {}
        start_input_thread(midi_msgs.clone(), midi_in, in_port, reset.clone());
        let patch_table = Arc::new(
            // todo: make the function search for all in patches/*.toml
            create_ordered_patch_table(&["patches/patches.toml"], &"order.toml", &global_config),
        );
        start_output_thread::<10>(
            midi_msgs.clone(),
            patch_table.clone(),
            Option::from(global_config),
        );
        run_chooser(midi_msgs, patch_table, reset.clone(), &mut quit);
    }
    Ok(())
}

fn run_chooser(
    midi_msgs: Arc<SegQueue<SynthMsg>>,
    patch_table: Arc<PatchTable>,
    reset: Arc<AtomicCell<bool>>,
    quit: &mut bool,
) {
    let main_menu = vec!["Pick New Synthesizer Sound", "Pick New MIDI Device", "Quit"];
    while !*quit && !reset.load() {
        println!("Play notes at will. When ready for a change, select one of the following:");
        match console_choice_from("Choice", &main_menu, |s| *s) {
            0 => {
                let program = {
                    let patch_table = patch_table.clone();
                    console_choice_from("Change synth to", &patch_table.entries, |opt| {
                        opt.name.as_str()
                    })
                };
                midi_msgs.push(SynthMsg::patch_change(program as u8, Speaker::Both));
            }
            1 => reset.store(true),
            2 => *quit = true,
            _ => panic!("This should never happen."),
        }
    }
}
