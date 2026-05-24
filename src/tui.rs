use anyhow::{anyhow, bail};
use midir::{Ignore, MidiInput, MidiInputPort};
use read_input::InputBuild;
use read_input::shortcut::input;

/// Returns a handle to the first MIDI device detected.
pub fn get_first_midi_device(midi_in: &mut MidiInput) -> anyhow::Result<MidiInputPort> {
    midi_in.ignore(Ignore::None);
    let in_ports = midi_in.ports();
    let mut device_name = None;
    if in_ports.is_empty() {
        bail!("No MIDI devices attached")
    } else {
        for (idx, port) in in_ports.iter().enumerate() {
            let current = midi_in.port_name(port);
            if let Ok(name) = current {
                if !name.to_lowercase().contains("thru") & !name.to_lowercase().contains("through")
                {
                    device_name = Some(name);
                    let device_name =
                        device_name.ok_or_else(|| anyhow!("No usable MIDI device"))?;
                    println!("Chose MIDI device {device_name}");
                    return Ok(in_ports[idx].clone());
                }
            }
        }
        Err(anyhow!("No MIDI devices found"))
    }
}

/// Allows selecting a MIDI device via the console from a complete list of MIDI devices.
/// The basic concept can be a model of how to do this in a GUI setting.
pub fn choose_midi_device(midi_in: &mut MidiInput) -> anyhow::Result<MidiInputPort> {
    midi_in.ignore(Ignore::None);
    let in_ports = midi_in.ports();
    match in_ports.len() {
        0 => bail!("No MIDI devices attached"),
        1 => get_first_midi_device(midi_in),
        _ => {
            let mut choices = vec![];
            for port in in_ports.iter() {
                choices.push((midi_in.port_name(port)?, port));
            }
            let c = console_choice_from("Select MIDI Device", &choices, |choice| choice.0.as_str());
            Ok(choices[c].1.clone())
        }
    }
}

/// Presents a list of items to be selected via console input. Used in multiple
/// [example](https://github.com/gjf2a/nabi_core/tree/master/examples) programs.
pub fn console_choice_from<T, F: Fn(&T) -> &str>(
    prompt: &str,
    choices: &Vec<T>,
    prompt_func: F,
) -> usize {
    for i in 0..choices.len() {
        println!("{}: {}", i + 1, prompt_func(&choices[i]));
    }
    let prompt = format!("{prompt}: ");
    input().msg(prompt).inside(1..=choices.len()).get() - 1
}
