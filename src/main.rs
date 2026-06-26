use crossbeam_queue::SegQueue;
use crossbeam_utils::atomic::AtomicCell;
use midir::MidiInput;
use nabi_core::config_builder::{
    create_ordered_patch_table, gather_toml_files_recursive, load_global_config,
};
use nabi_core::ios::synth::SynthMsg;
use nabi_core::ios::threading::{get_first_input_port, start_input_thread, start_output_thread};
use nabi_core::patch_builder::PatchTable;
use nabi_core::tui::console_choice_from;
use std::sync::Arc;

fn main() -> anyhow::Result<()> {
    let reset = Arc::new(AtomicCell::new(false));
    let mut quit = false;
    while !quit {
        let global_config = load_global_config("config/global.toml");
        let mut midi_in = MidiInput::new("midir reading input")?;
        let in_port = get_first_input_port(&mut midi_in);
        let midi_msgs = Arc::new(SegQueue::new());
        while reset.load() {}
        start_input_thread(midi_msgs.clone(), midi_in, in_port, reset.clone());
        let patch_paths = gather_toml_files_recursive(&global_config.patches_path);
        let patch_table = Arc::new(create_ordered_patch_table(
            patch_paths,
            &"config/order.toml",
        ));
        start_output_thread::<6>(
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
        if std::env::var("NABI_HEADLESS").is_ok() {
            std::thread::sleep(std::time::Duration::from_millis(500));
        } else {
            println!("Play notes at will. When ready for a change, select one of the following:");
            std::thread::sleep(std::time::Duration::from_millis(500));
            match console_choice_from("Choice", &main_menu, |s| *s) {
                0 => {
                    let program = {
                        let patch_table = patch_table.clone();
                        console_choice_from("Change synth to", &patch_table.entries, |opt| {
                            opt.toml.name.as_str()
                        })
                    };
                    midi_msgs.push(SynthMsg::patch_change(program as u8));
                }
                1 => reset.store(true),
                2 => *quit = true,
                _ => panic!("This should never happen."),
            }
        }
    }
}
