use fundsp::prelude64::*;
use oximedia_effects::stereo_widener::{StereoWidener, WidenerMode};


#[derive(Clone)]
pub struct OxiStereoWidenerNode {
    inner: StereoWidener,
}

impl OxiStereoWidenerNode {
    pub fn new(width: f32, mode: WidenerMode) -> Self {
        Self {
            inner: StereoWidener::new(mode, width),
        }
    }
    pub fn set_width(&mut self, width: f32) { self.inner.set_width(width); }
    pub fn set_mode(&mut self, mode: WidenerMode) { self.inner.set_mode(mode); }
}

// This is where we implement the FunDSP magic to make our struct a true AudioNode.
impl AudioNode for OxiStereoWidenerNode {
    const ID: u64 = 42467;
    type Inputs = U3; // Takes stereo input  + width variable
    type Outputs = U2; // Produces stereo output

    #[inline]
    fn tick(&mut self, input: &Frame<f32, Self::Inputs>) -> Frame<f32, Self::Outputs> {
        // 1. Get the left and right samples from the input frame.
        let left = input[0];
        let right = input[1];
        let width = input.get(3).copied().unwrap_or(1.2);
        self.set_width(width);
        let (new_left, new_right) = self.inner.process_sample(left, right);
        [new_left, new_right].into()
    }
}

fn stereo_widener(width: f32, mode: WidenerMode) -> An<OxiStereoWidenerNode> {
    An(OxiStereoWidenerNode::new(width, mode))
}

