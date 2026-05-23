use crate::common_definitions::params::{ParamDefault, ParamInfo, ParamType, Parameterized};
use fundsp::audiounit::AudioUnit;
use fundsp::prelude64::{constant, poly_pulse, poly_saw, poly_square, sine, triangle};
use serde::{Deserialize, Deserializer};

fn deserialize_polarity_type<'de, D>(deserializer: D) -> Result<Polarity, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    match s.to_lowercase().as_str() {
        "positive" => Ok(Polarity::Positive),
        "negative" => Ok(Polarity::Negative),
        _ => Err(serde::de::Error::unknown_variant(
            &s,
            &["positive", "negative"],
        )),
    }
}

pub enum Polarity {
    Positive,
    Negative,
}

impl Polarity {
    pub(crate) fn to_float(&self) -> f32 {
        match self {
            Polarity::Positive => 1.0,
            Polarity::Negative => -1.0,
        }
    }
}

fn deserialize_osc_type<'de, D>(deserializer: D) -> Result<OscillatorType, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    match s.to_lowercase().as_str() {
        "saw" => Ok(OscillatorType::Saw),
        "triangle" => Ok(OscillatorType::Triangle),
        "sine" => Ok(OscillatorType::Sine),
        "pulse" => Ok(OscillatorType::Pulse),
        "square" => Ok(OscillatorType::Square),
        "none" => Ok(OscillatorType::None),
        _ => Err(serde::de::Error::unknown_variant(
            &s,
            &["saw", "triangle", "sine", "pulse", "square", "none"],
        )),
    }
}

#[derive(serde::Deserialize)]
pub enum OscillatorType {
    Saw,
    Triangle,
    Sine,
    Pulse, // todo: add Pulse Width
    Square,
    None,
}

impl OscillatorType {
    pub fn to_audiounit(&self) -> Box<dyn AudioUnit> {
        match self {
            OscillatorType::Saw => Box::new(poly_saw()),
            OscillatorType::Triangle => Box::new(triangle()),
            OscillatorType::Sine => Box::new(sine()),
            OscillatorType::Pulse => Box::new(poly_pulse()),
            OscillatorType::Square => Box::new(poly_square()),
            OscillatorType::None => Box::new(sine() * constant(0.0)),
        }
    }
}

#[derive(Deserialize)]
#[serde(default)]
pub struct TwoOscMorphParams {
    #[serde(deserialize_with = "deserialize_osc_type")]
    pub oscillator_type_1: OscillatorType,
    #[serde(deserialize_with = "deserialize_osc_type")]
    pub oscillator_type_2: OscillatorType,
}

impl Default for TwoOscMorphParams {
    fn default() -> TwoOscMorphParams {
        TwoOscMorphParams {
            oscillator_type_1: OscillatorType::Triangle,
            oscillator_type_2: OscillatorType::Sine,
        }
    }
}

impl Parameterized for TwoOscMorphParams {
    fn param_info() -> &'static [ParamInfo] {
        &[ParamInfo {
            name: "balance",
            param_type: ParamType::ZeroToOneFloat,
            default: ParamDefault::ZeroToOneFloat(0.5),
            description: None,
        }]
    }
}
