use crossbeam_queue::SegQueue;
use crossbeam_utils::atomic::AtomicCell;
use midir::MidiInput;
use nabi_core::config_builder::MAX_KNOBS_PER_GROUP;
use nabi_core::ios::threads::{cc_mapper_handler, get_first_input_port, start_input_thread};
use std::sync::Arc;

fn main() -> anyhow::Result<()> {
    let quit_cc_mapper = Arc::new(AtomicCell::new(false));
    let quit_program = Arc::new(AtomicCell::new(false));

    let mut midi_in = MidiInput::new("midir reading input")?;
    let in_port = get_first_input_port(&mut midi_in);
    let midi_msgs = Arc::new(SegQueue::new());
    while quit_program.load() {}
    start_input_thread(midi_msgs.clone(), midi_in, in_port, quit_program.clone());
    let mut fx_cc: Vec<u8> = Vec::with_capacity(MAX_KNOBS_PER_GROUP);
    let mut dummy_string = String::new();
    println!("Welcome to Nabi CC automapper tool!");
    for idx in 0..MAX_KNOBS_PER_GROUP {
        quit_cc_mapper.store(false);
        while quit_cc_mapper.load() {}
        println!(
            "Move the CC controller to map ** FX ** control number {}",
            idx + 1
        );
        println!(
            "Press Enter to finalize. If no control was activated we will conclude the FX Mapper and continue to Sounds..."
        );
        let cc = cc_mapper_handler(midi_msgs.clone(), quit_cc_mapper.clone());
        std::io::stdin().read_line(&mut dummy_string)?;
        quit_cc_mapper.store(true);
        if let Some(num) = cc.join().expect("problem joining thread!") {
            fx_cc.push(num);
            println!("Current FX mapping: {:?}", fx_cc);
        } else {
            quit_cc_mapper.store(false);
            break;
        }
    }
    println!("Complete FX mapping: {:?}", fx_cc);
    // do sound...
    quit_program.store(true);
    Ok(())
}
