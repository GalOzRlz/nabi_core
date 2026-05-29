use fundsp::Frame;
use fundsp::audionode::Map;
use fundsp::prelude64::{An, U1, map};

pub fn quantize_u8_to_01(value: u8) -> f32 {
    let norm = value as f32 / 127.0;
    ((norm * 100.0).round().clamp(0.0, 100.0) as i32) as f32 / 100.0
}

/// Quantizes 0.0-1.0 values into 0.1 steps
pub fn quantize_01_step() -> An<Map<fn(&Frame<f32, U1>) -> f32, U1, f32>> {
    map(|i: &Frame<f32, U1>| (i[0] * 100_f32).round() / 100_f32)
}
