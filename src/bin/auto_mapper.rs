use crossbeam_queue::SegQueue;
use crossbeam_utils::atomic::AtomicCell;
use midir::MidiInput;
use nabi_core::config_builder::{GlobalConfigToml, GlobalSection, MAX_KNOBS_PER_GROUP};
use nabi_core::ios::midi::SynthMsg;
use nabi_core::ios::threads::{cc_mapper_handler, get_first_input_port, start_input_thread};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, mpsc};

fn main() -> anyhow::Result<()> {
    let quit_program = Arc::new(AtomicCell::new(false));

    let mut midi_in = MidiInput::new("midir reading input")?;
    let in_port = get_first_input_port(&mut midi_in);
    let midi_msgs = Arc::new(SegQueue::new());
    while quit_program.load() {}
    start_input_thread(midi_msgs.clone(), midi_in, in_port, quit_program.clone());
    let mut fx_cc = Vec::with_capacity(MAX_KNOBS_PER_GROUP);
    let mut sound_cc = Vec::with_capacity(MAX_KNOBS_PER_GROUP);
    let mut left_right = [0u8; 2];
    println!("Welcome to Nabi CC automapper tool!");
    for idx in 0..MAX_KNOBS_PER_GROUP {
        println!(
            "Move the CC controller to map ** FX ** control number {}",
            idx + 1
        );
        println!(
            "Press Enter to finalize. If no control was activated we will continue to Left-Right mapping..."
        );
        if let Some(num) = handle_cc_events_mapper(midi_msgs.clone()) {
            fx_cc.push(num);
            println!("Current FX mapping: {:?}", fx_cc);
        } else {
            break;
        }
    }
    println!("Complete FX mapping: {:?}", fx_cc);
    for idx in 0..MAX_KNOBS_PER_GROUP {
        println!(
            "Move the CC controller to map ** SOUND ** control number {}",
            idx + 1
        );
        println!(
            "Press Enter to finalize. If no control was activated we will conclude the mapping utility and continue to write to config file"
        );
        if let Some(num) = handle_cc_events_mapper(midi_msgs.clone()) {
            sound_cc.push(num);
            println!("Current SOUND mapping: {:?}", sound_cc);
        } else {
            break;
        }
    }
    println!("Press your controllers LEFT button and then Enter...");
    if let Some(num) = handle_cc_events_mapper(midi_msgs.clone()) {
        left_right[0] = num;
        println!("LEFT mapping: {:?}", num);
    }
    println!("Press your controllers RIGHT button and then Enter...");
    if let Some(num) = handle_cc_events_mapper(midi_msgs.clone()) {
        left_right[1] = num;
        println!("RIGHT mapping: {:?}", num);
    }
    quit_program.store(true);
    let mut global_toml_section = GlobalSection::default();
    global_toml_section.left_right_buttons = Option::from(left_right);
    global_toml_section.fx_cc_mapping = Option::from(fx_cc);
    global_toml_section.sound_cc_mapping = Option::from(sound_cc);
    let toml = GlobalConfigToml {
        global: global_toml_section,
    };
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("config/global.toml".to_string());
    let new_toml =
        toml::to_string_pretty(&toml).expect("failed to serialize global settings to TOML");
    fs::write(&path, &new_toml).expect("failed to save patch state to TOML");
    println!("Wrote settings file to {:?}", path);
    println!("Wrote patch state to TOML\n{}", new_toml);
    println!("Exiting utility!");
    Ok(())
}

fn handle_cc_events_mapper(midi_msgs: Arc<SegQueue<SynthMsg>>) -> Option<u8> {
    let mut dummy_string = String::new();
    let (data_tx, data_rx) = mpsc::channel();
    let (stop_tx, stop_rx) = mpsc::channel();
    let cc = cc_mapper_handler(midi_msgs.clone(), data_tx, stop_rx);
    std::io::stdin()
        .read_line(&mut dummy_string)
        .expect("problem with reading console input!");
    let _ = stop_tx.send(None);
    cc.join().unwrap();
    data_rx.recv().unwrap_or(None)
}
