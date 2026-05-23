use fundsp::combinator::An;
use fundsp::follow::Follow;
use fundsp::prelude64::follow;

pub fn cc_smooth() -> An<Follow<f64>> {
    follow(0.005)
}
