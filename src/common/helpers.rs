use fundsp::prelude32::{ShapeFn, Shaper};
use fundsp::prelude64::{shape_fn, An};

pub fn quantize_u8_to_01(value: u8) -> f32 {
    let norm = value as f32 / 127.0;
    ((norm * 100.0).round().clamp(0.0, 100.0) as i32) as f32 / 100.0
}

/// Quantizes 0.0-1.0 values into 0.01 steps
pub fn quantize_01_decimal() -> An<Shaper<ShapeFn<fn(f32) -> f32>>> {
    shape_fn(|i| (i * 100_f32).round() / 100_f32)
}
