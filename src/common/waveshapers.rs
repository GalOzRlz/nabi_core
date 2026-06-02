use fundsp::combinator::An;
use fundsp::prelude32::{ShapeFn, Shaper};
use fundsp::prelude64::shape_fn;

pub fn rectify() -> An<Shaper<ShapeFn<fn(f32) -> f32>>> {
    shape_fn(move |x| {
        let threshold = 0.5;
        if x > threshold {
            threshold - (x - threshold)
        } else if x < -threshold {
            -threshold - (x + threshold)
        } else {
            x
        }
    })
}
