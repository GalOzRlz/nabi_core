use crate::SharedMidiState;
use crate::common::helpers::quantize_u8_to_01;
use crate::config_builder::{ConfigurableMapping, MAX_KNOBS_PER_GROUP};
use crate::helpers::fundsp::to_net;
use anyhow::anyhow;
use fundsp::audionode::Pipe;
use fundsp::follow::Follow;
use fundsp::prelude64::{
    An, AudioUnit, Net, U1, U2, Unit, Var, Wave, WaveSynth, Wavetable, adsr_live, join, pass,
    poly_saw, poly_square, pulse, sine, triangle, unit,
};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use toml::Value;

pub type CcAudioNode = An<Pipe<Var, Follow<f64>>>;
pub type CcArray = [f32; MAX_KNOBS_PER_GROUP];

impl ToNet for CcAudioNode {
    fn to_net(self) -> Net {
        to_net(self)
    }
}

pub trait ToNet {
    fn to_net(self) -> Net;
}
pub trait CcInit {
    fn get_initial_cc(&self) -> CcArray;
}

fn to_mono_unit(audiounit: Box<dyn AudioUnit>) -> An<Unit<U1, U1>> {
    unit::<U1, U1>(audiounit)
}

fn stereo_to_mono_unit(audiounit: Box<dyn AudioUnit>) -> An<Unit<U2, U1>> {
    unit::<U2, U1>(audiounit)
}

#[derive(Debug, Clone)]
pub enum ParamType {
    U8(u8),
    Oscillator(Cow<'static, str>),
    ZeroOneFloat(f32),
    ZeroHundredFloat(f32),
    ADSR([f32; 4]),
    Noise(Cow<'static, str>),
}

impl ParamType {
    pub fn as_array(&self) -> anyhow::Result<[f32; 4]> {
        match &self {
            ParamType::ADSR(a) => Ok(*a),
            _ => Err(anyhow!("cannot convert param to array")),
        }
    }

    pub fn as_zero_to_one_f32(&self) -> Result<f32, anyhow::Error> {
        match &self {
            ParamType::U8(v) => Ok(quantize_u8_to_01(*v)),
            ParamType::Oscillator(_) => Err(anyhow!("ParamType::Oscillator has no numeric value!")),
            ParamType::ADSR(_) => Err(anyhow!("ParamType::ADSR has no numeric value!")),
            ParamType::ZeroOneFloat(v) => Ok(v.clamp(0.0, 1.0)),
            ParamType::ZeroHundredFloat(v) => Ok((*v / 100.0).clamp(0.0, 1.0)), // scale to 0.0-1.0
            ParamType::Noise(_) => Err(anyhow!("ParamType::Noise has no numeric value!")),
        }
    }

    pub fn as_f32(&self) -> Result<f32, anyhow::Error> {
        match &self {
            ParamType::U8(v) => Ok(*v as f32),
            ParamType::Oscillator(_) => Err(anyhow!("ParamType::Oscillator has no numeric value!")),
            ParamType::ADSR(_) => Err(anyhow!("ParamType::ADSR has no numeric value!")),
            ParamType::ZeroOneFloat(v) => Ok(v.clamp(0.0, 1.0)),
            ParamType::ZeroHundredFloat(v) => Ok(*v),
            ParamType::Noise(_) => Err(anyhow!("ParamType::Noise has no numeric value!")),
        }
    }

    pub fn as_oscillator_type(&self) -> Result<OscillatorType, &'static str> {
        match self {
            ParamType::Oscillator(s) => OscillatorType::from_str(s),
            _ => Err("parameter is not a string, cannot convert to oscillator type"),
        }
    }
}

impl std::fmt::Display for ParamType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParamType::U8(v) => write!(f, "{}", v),
            ParamType::Oscillator(s) => write!(f, "{}", s),
            ParamType::ZeroOneFloat(v) => write!(f, "{}", v),
            ParamType::ZeroHundredFloat(v) => write!(f, "{}", v),
            ParamType::ADSR(v) => write!(f, "{:?}", v),
            ParamType::Noise(v) => write!(f, "{:?}", v),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CcParam {
    pub value: ParamType,
    pub cc_index: usize,
    pub name: &'static str,
    pub description: Option<&'static str>,
}

#[derive(Debug, Clone)]
pub struct NonCcParam {
    pub value: ParamType,
    pub name: &'static str,
    pub description: Option<&'static str>,
}

#[derive(Clone, Debug)]
pub struct Parameterized {
    pub name: &'static str,
    pub cc_params: Option<Cow<'static, [CcParam]>>,
    pub non_cc_params: Option<Cow<'static, [NonCcParam]>>,
}

impl CcInit for Parameterized {
    fn get_initial_cc(&self) -> [f32; MAX_KNOBS_PER_GROUP] {
        let mut cc_array = [0_f32; MAX_KNOBS_PER_GROUP];
        if let Some(cc_params_cow) = &self.cc_params {
            for cc_param in cc_params_cow.iter() {
                //println!("param: {} with index: {}", cc_param.name, cc_param.cc_index);
                if cc_param.cc_index >= 1 {
                    cc_array[cc_param.cc_index - 1] = cc_param.value.as_zero_to_one_f32().unwrap()
                }
            }
        }
        //        println!("name: {} with index: {:?}", self.name, cc_array);
        cc_array
    }
}

impl Parameterized {
    pub fn apply_toml_overrides<T>(&mut self, toml_config: &T)
    where
        T: ConfigurableMapping,
    {
        if let Some(cfg) = toml_config.get_values() {
            if let Some(mut_cc_params) = self.cc_params.as_mut() {
                apply_toml_values_overrides(mut_cc_params.to_mut(), &cfg);
            }
            if let Some(mut_non_cc_params) = self.non_cc_params.as_mut() {
                apply_toml_values_overrides(mut_non_cc_params.to_mut(), &cfg);
            }
        }
        if let Some(user_mappings) = toml_config.get_mapping() {
            apply_toml_mapping(self, user_mappings);
        }
    }

    /// Returns an ADSR envelope in a `Box` based on internal parameters.
    pub fn boxed_adsr(&self, param_name: &str, state: &SharedMidiState) -> Box<dyn AudioUnit> {
        let control = state.control_var();
        if let Ok(param) = self.get_non_cc_param(param_name) {
            Box::new(
                // todo: make into a generic struct that returns boxed audiounit
                control
                    >> adsr_live(
                        param.value.as_array().unwrap()[0],
                        param.value.as_array().unwrap()[1],
                        param.value.as_array().unwrap()[2],
                        param.value.as_array().unwrap()[3],
                    ),
            )
        } else {
            // default:
            Box::new(control >> adsr_live(0.1, 0.0, 1.0, 0.3))
        }
    }

    pub fn get_cc_param(&self, name: &str) -> anyhow::Result<&CcParam> {
        if let Some(vec) = &self.cc_params {
            for i in vec.iter() {
                if i.name == name {
                    return Ok(i);
                }
            }
        }
        Err(anyhow!(format!("CC-Parameter {} not found", name)))
    }

    pub fn to_config_values(&self) -> HashMap<String, Value> {
        let mut new_map: HashMap<String, Value> = HashMap::new();
        if let Some(params) = self.cc_params.as_ref() {
            for def in params.iter() {
                new_map.insert(
                    def.name.to_string(),
                    Value::Float(def.value.as_f32().unwrap() as f64),
                );
            }
        };
        new_map
    }
    pub fn apply_cc_state(&self, cc_array: &CcArray) {
        todo!()
    }
    pub fn set_cc_param(&mut self, name: &str, value: f32) {
        todo!()
    }

    pub fn cc_sound_or_default(&self, name: &str, shared: &SharedMidiState) -> CcAudioNode {
        shared.get_sound_an_or_default(&self.get_cc_param(name).unwrap())
    }

    pub fn cc_fx_or_default(&self, name: &str, shared: &SharedMidiState) -> CcAudioNode {
        shared.get_fx_an_or_default(&self.get_cc_param(name).unwrap())
    }

    pub fn get_non_cc_param(&self, name: &str) -> anyhow::Result<&NonCcParam> {
        if let Some(vec) = &self.non_cc_params {
            for i in vec.iter() {
                if i.name.to_lowercase() == name.to_lowercase() {
                    return Ok(i);
                }
            }
        }
        Err(anyhow!(format!("Non-CC-Parameter {} not found", name)))
    }
    pub fn get_osc_node_type(&self, name: &str) -> anyhow::Result<OscillatorType> {
        let param = self
            .get_non_cc_param(name)
            .map_err(|_| anyhow::anyhow!("parameter not found"))?;
        param
            .value
            .as_oscillator_type()
            .map_err(|e| anyhow::anyhow!(e))
    }

    pub fn get_noise_node_type(&self, name: &str) -> anyhow::Result<NoiseType> {
        todo!()
    }
}

pub trait ValuedParam {
    fn get_mut(&mut self) -> &mut ParamType;

    fn get_name(&self) -> &str;
}

impl ValuedParam for CcParam {
    fn get_mut(&mut self) -> &mut ParamType {
        &mut self.value
    }

    fn get_name(&self) -> &str {
        self.name
    }
}

impl ValuedParam for NonCcParam {
    fn get_mut(&mut self) -> &mut ParamType {
        &mut self.value
    }

    fn get_name(&self) -> &str {
        self.name
    }
}

pub fn apply_toml_values_overrides<T>(params: &mut [T], toml_overrides: &HashMap<String, Value>)
where
    T: ValuedParam,
{
    for param in params {
        if let Some(toml_value) = toml_overrides.get(param.get_name()) {
            match param.get_mut() {
                ParamType::ZeroOneFloat(v) | ParamType::ZeroHundredFloat(v) => {
                    if let Some(num) = toml_value.as_float() {
                        *v = num as f32;
                    }
                }
                ParamType::U8(v) => {
                    if let Some(num) = toml_value.as_integer() {
                        *v = num as u8;
                    } else if let Some(num) = toml_value.as_float() {
                        *v = num as u8;
                    }
                }
                ParamType::Oscillator(s) | ParamType::Noise(s) => {
                    if let Some(str_val) = toml_value.as_str() {
                        *s = osc_string_to_cow(str_val);
                    }
                }
                ParamType::ADSR(array) => {
                    if let Some(new_array) = toml_value.as_array() {
                        for (idx, val) in new_array.iter().enumerate() {
                            array[idx] = val.as_float().unwrap_or(0.0) as f32;
                        }
                    }
                }
            }
        }
    }
}

pub fn apply_toml_mapping(params: &mut Parameterized, toml_mapping: &HashMap<String, Value>) {
    if let Some(ref mut cc_params) = params.cc_params {
        let params_mut = cc_params.to_mut();
        for param in params_mut.iter_mut() {
            if let Some(val) = toml_mapping.get(param.name).and_then(|v| v.as_integer()) {
                param.cc_index = val as usize
            }
        }
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

#[derive(serde::Deserialize)]
pub enum NoiseType {
    White,
    Pink,
    Brown,
}

#[derive(serde::Deserialize)]
pub enum OscillatorType {
    Saw,
    Triangle,
    Sine,
    Pulse,
    Square,
    WaveTable(String),
    None,
}

impl FromStr for OscillatorType {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lower = s.to_lowercase();
        match lower.as_str() {
            "saw" => Ok(OscillatorType::Saw),
            "triangle" => Ok(OscillatorType::Triangle),
            "sine" => Ok(OscillatorType::Sine),
            "pulse" => Ok(OscillatorType::Pulse),
            "square" => Ok(OscillatorType::Square),
            "none" => Ok(OscillatorType::None),
            // assuming that other text is for wavetable path:
            file_path => Ok(OscillatorType::WaveTable(file_path.parse().unwrap())),
        }
    }
}
fn osc_string_to_cow(s: &str) -> Cow<'static, str> {
    match s.to_lowercase().as_str() {
        "saw" => Cow::Borrowed("saw"),
        "triangle" => Cow::Borrowed("triangle"),
        "sine" => Cow::Borrowed("sine"),
        "pulse" => Cow::Borrowed("pulse"),
        "square" => Cow::Borrowed("square"),
        "none" => Cow::Borrowed("none"),
        // Any other string (file path, custom name) – take ownership
        other => Cow::Owned(other.to_string()),
    }
}

impl OscillatorType {
    pub fn get_osc_node(&self) -> An<Unit<U1, U1>> {
        match self {
            OscillatorType::Saw => to_mono_unit(Box::new(poly_saw())),
            OscillatorType::Triangle => to_mono_unit(Box::new(triangle())),
            OscillatorType::Sine => to_mono_unit(Box::new(sine())),
            OscillatorType::Pulse => to_mono_unit(Box::new(poly_square())),
            OscillatorType::Square => to_mono_unit(Box::new(poly_square())),
            OscillatorType::None => to_mono_unit(Box::new(sine() * 0.0)),
            OscillatorType::WaveTable(s) => to_mono_unit(Self::path_to_wavetable_synth(s)),
        }
    }

    fn path_to_wavetable_synth(path: &str) -> Box<An<WaveSynth<U1>>> {
        let mut wav_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        wav_path.push(path);
        let wave = Wave::load(wav_path).expect("Failed to load WAV file for wavetable synth!");
        let wavetable = Wavetable::from_wave(20.0, 20000.0, 12.0, wave.channel(0));
        let synth = WaveSynth::new(Arc::new(wavetable));
        Box::new(An(synth))
    }

    pub fn get_osc_node_pw(&self) -> An<Unit<U2, U1>> {
        // nullify the second value for osc that don't support pulse width
        let pw_sinker = (pass() | pass() * 0.0) >> join::<U2>();
        match self {
            OscillatorType::Saw => stereo_to_mono_unit(Box::new(pw_sinker >> self.get_osc_node())),
            OscillatorType::Triangle => {
                stereo_to_mono_unit(Box::new(pw_sinker >> self.get_osc_node()))
            }
            OscillatorType::Sine => stereo_to_mono_unit(Box::new(pw_sinker >> self.get_osc_node())),
            OscillatorType::Pulse => stereo_to_mono_unit(Box::new(pulse())),
            OscillatorType::Square => {
                stereo_to_mono_unit(Box::new(pw_sinker >> self.get_osc_node()))
            }
            OscillatorType::None => stereo_to_mono_unit(Box::new(pw_sinker >> sine() * 0.0)),
            _ => panic!("Type cannot accept any inputs - therefore cannot force it to receive "),
        }
    }
}

impl FromStr for NoiseType {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<NoiseType, &'static str> {
        let lower = s.to_lowercase();
        match lower.as_str() {
            "white" => Ok(NoiseType::White),
            "brown" => Ok(NoiseType::Brown),
            "pink" => Ok(NoiseType::Pink),
            _ => Err("Unrecognized noise type"),
        }
    }
}
