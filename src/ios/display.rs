use midir::{MidiOutput, MidiOutputConnection};

pub struct KeyboardDisplay {
    conn: MidiOutputConnection,
    model: KeyboardModel,
}

enum KeyboardModel {
    KeyLabEssentialMk3,
}

impl KeyboardDisplay {
    pub fn try_new() -> Option<Self> {
        let midi_out = MidiOutput::new("Nabi Screen Control").ok()?;

        for port in midi_out.ports() {
            let name = midi_out.port_name(&port).ok()?;

            if name.to_lowercase().contains("kl essential")
                && name.to_lowercase().contains("mk3")
                && name.to_lowercase().contains("alv")
            {
                let model = KeyboardModel::KeyLabEssentialMk3;
                println!("connected to port {:?}", name);
                let mut conn = midi_out.connect(&port, "nabi-screen-out").ok()?;
                let handshake = vec![
                    0xF0, 0x00, 0x20, 0x6B, 0x7F, 0x42, 0x02, 0x0F, 0x40, 0x5A, 0x01, 0xF7,
                ];
                conn.send(&handshake)
                    .expect("failed to send screen-enable handshake!");

                return Some(KeyboardDisplay { conn, model });
            } else {
                continue;
            };
        }
        None
    }

    pub fn set_text(&mut self, line1: &str, line2: &str) -> anyhow::Result<()> {
        let msg = self.build_sysex(line1, line2);
        self.conn.send(&msg)?;
        Ok(())
    }

    pub fn clear_screen(&mut self) -> anyhow::Result<()> {
        match self.model {
            KeyboardModel::KeyLabEssentialMk3 => {
                let clear = vec![
                    0xF0, 0x00, 0x20, 0x6B, 0x7F, 0x42, 0x04, 0x01, 0x60, 0x61, 0xF7,
                ];
                self.conn.send(&clear)?;
            }
        }

        Ok(())
    }

    fn build_sysex(&self, line1: &str, line2: &str) -> Vec<u8> {
        match self.model {
            KeyboardModel::KeyLabEssentialMk3 => {
                let mut msg = vec![0xF0, 0x00, 0x20, 0x6B, 0x7F, 0x42];
                // 2L command: two centered lines, permanent (transient = 00)
                msg.extend_from_slice(&[0x04, 0x01, 0x60, 0x12, 0x01]);

                // Line 1: pad/truncate to 18 bytes
                let mut l1 = line1.as_bytes().to_vec();
                l1.resize(18, 0x00);
                msg.extend(&l1);

                // Delimiter
                msg.push(0x00);
                msg.push(0x02);

                // Line 2: 18 bytes
                let mut l2 = line2.as_bytes().to_vec();
                l2.resize(18, 0x00);
                msg.extend(&l2);

                // Terminator and footer
                msg.push(0x00);
                msg.push(0xF7);

                msg
            }
        }
    }
}

pub fn shorten_cc_name(name: &str) -> &str {
    // todo: something better..
    if name.len() > 14 {
        return name.split('_').next().unwrap_or(&name[0..14]);
    }
    name
}
