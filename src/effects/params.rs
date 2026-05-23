use crate::common_definitions::params::{ParamDefault, ParamInfo, ParamType, Parameterized};
use serde::Deserialize;

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
            room_size: 7.8,
            damping: 0.5,
            length: 3.5,
        }
    }
}

impl Parameterized for ReverbParams {
    fn param_info() -> &'static [ParamInfo] {
        &[
            ParamInfo {
                name: "Room Size",
                param_type: ParamType::Float,
                default: ParamDefault::Float(0.8),
                description: Some("The size of the simulated room"),
            },
            ParamInfo {
                name: "Damping",
                param_type: ParamType::Float,
                default: ParamDefault::Float(0.5),
                description: Some(
                    "How much higher frequency suppression will occur in the reverb over time",
                ),
            },
        ]
    }
}
