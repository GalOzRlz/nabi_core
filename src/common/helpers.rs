use fundsp::audiounit::{AudioUnit, Unit};
use fundsp::prelude32::{ShapeFn, Shaper};
use fundsp::prelude64::{An, U0, U1, U2, shape_fn, unit};

pub fn quantize_u8_to_01(value: u8) -> f32 {
    let norm = value as f32 / 127.0;
    ((norm * 100.0).round().clamp(0.0, 100.0) as i32) as f32 / 100.0
}

/// Quantizes 0.0-1.0 values into 0.01 steps
pub fn quantize_01_decimal() -> An<Shaper<ShapeFn<fn(f32) -> f32>>> {
    shape_fn(|i| (i * 100_f32).round() / 100_f32)
}

pub fn to_mono_unit(audiounit: Box<dyn AudioUnit>) -> An<Unit<U1, U1>> {
    unit::<U1, U1>(audiounit)
}

pub fn to_zero_mono_unit(audiounit: Box<dyn AudioUnit>) -> An<Unit<U0, U1>> {
    unit::<U0, U1>(audiounit)
}

pub fn to_stereo_unit(audiounit: Box<dyn AudioUnit>) -> An<Unit<U2, U2>> {
    unit::<U2, U2>(audiounit)
}

pub fn stereo_to_mono_unit(audiounit: Box<dyn AudioUnit>) -> An<Unit<U2, U1>> {
    unit::<U2, U1>(audiounit)
}
