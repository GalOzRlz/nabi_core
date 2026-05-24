use crate::effects::effects_building::FxChainFactory;
use crate::sound_engine::sound_building::SoundFactory;
use crate::tunings::TunerBuilder;
use std::collections::HashMap;

pub type CcMap = HashMap<String, usize>;
// ---- Knob labels (shared with effects_builders) ----
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnobGroup {
    Sound,
    Effect,
}

#[derive(Debug, Clone)]
pub struct KnobLabel {
    pub group: KnobGroup,
    pub index: usize, // 1‑based logical knob
    pub label: String,
}

#[derive(Clone)]
pub struct PatchDef {
    pub sound_factory: SoundFactory,
    pub name: String,
    pub tuning: TunerBuilder,
    pub effects: FxChainFactory,
}

// ---- PatchTable ----
pub const NUM_PATCH_SLOTS: usize = 2_usize.pow(7);

#[derive(Clone)]
pub struct PatchTable {
    pub entries: Vec<PatchDef>,
}

impl PatchTable {
    pub fn new(entries: Vec<PatchDef>) -> Self {
        Self { entries }
    }
}
