pub(crate) use crate::common_definitions::params::CcParam;
use oximedia_effects::stereo_widener::WidenerMode;
use oximedia_effects::stereo_widener::WidenerMode::{HaasDelay, MidSide, PhaseSpread};
use serde::{Deserialize, Deserializer};

#[derive(Deserialize)]
#[serde(default)]
pub struct Eq2Params {
    pub(crate) lp_q: f32,
    pub(crate) hp_q: f32,
}

impl Default for Eq2Params {
    fn default() -> Self {
        Self {
            lp_q: 0.1,
            hp_q: 0.1,
        }
    }
}
pub struct ReverbParams {
    pub room_size: CcParam,
    pub damping: CcParam,
    pub length: CcParam,
}

fn stereo_widening_types<'de, D>(deserializer: D) -> Result<WidenerMode, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    match s.to_lowercase().as_str() {
        "midside" => Ok(MidSide),
        "haas" => Ok(HaasDelay),
        "phase_spread" => Ok(PhaseSpread),
        _ => Err(serde::de::Error::unknown_variant(
            &s,
            &["midside", "haas", "phase_spread"],
        )),
    }
}

#[derive(Deserialize)]
#[serde(default)]
pub struct WidenerParams {
    width: f32,
    #[serde(deserialize_with = "stereo_widening_types")]
    mode: WidenerMode,
}

impl Default for WidenerParams {
    fn default() -> Self {
        Self {
            width: 0.7,
            mode: PhaseSpread,
        }
    }
}
