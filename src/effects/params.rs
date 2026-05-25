use crate::common_definitions::params::{ParamDefault, ParamInfo, ParamType, Parameterized};
use oximedia_effects::stereo_widener::WidenerMode;
use oximedia_effects::stereo_widener::WidenerMode::{HaasDelay, MidSide, PhaseSpread};
use serde::{Deserialize, Deserializer};

#[derive(Deserialize)]
#[serde(default)]
pub struct NoParams {}

impl Default for NoParams {
    fn default() -> Self {
        Self {}
    }
}

impl Parameterized for NoParams {
    fn param_info() -> &'static [ParamInfo] {
        &[ParamInfo {
            name: "No parameter",
            param_type: ParamType::Float,
            default: ParamDefault::Float(0.0),
            description: None,
        }]
    }
}

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
impl Parameterized for Eq2Params {
    fn param_info() -> &'static [ParamInfo] {
        &[
            ParamInfo {
                name: "Low Pass Q",
                param_type: ParamType::Float,
                default: ParamDefault::Float(0.8),
                description: None,
            },
            ParamInfo {
                name: "High Pass Q",
                param_type: ParamType::Float,
                default: ParamDefault::Float(0.5),
                description: None,
            },
        ]
    }
}

#[derive(Deserialize)]
#[serde(default)]
pub struct ReverbParams {
    pub room_size: f32,
    pub damping: f32,
    pub length: f32,
}

impl Default for ReverbParams {
    fn default() -> Self {
        Self {
            room_size: 3.8,
            damping: 0.7,
            length: 1.5,
        }
    }
}

impl Parameterized for ReverbParams {
    fn param_info() -> &'static [ParamInfo] {
        &[
            ParamInfo {
                name: "room_size",
                param_type: ParamType::Float,
                default: ParamDefault::Float(0.8),
                description: Some("The size of the simulated room"),
            },
            ParamInfo {
                name: "damping",
                param_type: ParamType::ZeroToOneFloat,
                default: ParamDefault::ZeroToOneFloat(0.5),
                description: Some(
                    "How much higher frequency suppression will occur in the reverb over time",
                ),
            },
            ParamInfo {
                name: "length",
                param_type: ParamType::Float,
                default: ParamDefault::Float(0.5),
                description: Some(
                    "How much higher frequency suppression will occur in the reverb over time",
                ),
            },
        ]
    }
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
