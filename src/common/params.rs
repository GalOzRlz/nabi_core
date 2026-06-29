use crate::SharedMidiState;
use crate::common::envelopes::CcADSR;
use crate::common::helpers::{
    quantize_u8_to_01, stereo_to_mono_unit, to_mono_unit, to_zero_mono_unit,
};
use crate::config_builder::{ConfigurableMapping, MAX_KNOBS_PER_GROUP};
use anyhow::anyhow;
use fundsp::audionode::Pipe;
use fundsp::follow::Follow;
use fundsp::numeric_array::ArrayLength;
use fundsp::prelude64::{
    An, AudioUnit, U0, U1, U2, Unit, Var, Wave, WaveSynth, Wavetable, adsr_live, hammond, join,
    lorenz, organ, pass, poly_saw, poly_square, pulse, sine, triangle,
};
use fundsp::prelude64::{brown, pink, white};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use toml::Value;

pub type CcNode = An<Pipe<Var, Follow<f64>>>;

pub type PwNode = An<Unit<U2, U1>>;

pub type CcArray = [f32; MAX_KNOBS_PER_GROUP];

pub trait CcInit {
    fn get_initial_cc(&self) -> CcArray;
}

#[derive(Debug, Clone)]
pub enum ParamType {
    U8(u8),
    Oscillator(Cow<'static, str>),
    ZeroOneFloat(f32),
    ZeroTenFloat(f32),
    Float32(f32),
    ADSR([f32; 4]),
    Noise(Cow<'static, str>),
    String(Cow<'static, str>),
}

impl ParamType {
    pub fn with_cc_to_f32(&self, v: f32) -> ParamType {
        match self {
            ParamType::ADSR(_)
            | ParamType::Noise(_)
            | ParamType::Oscillator(_)
            | ParamType::String(_) => {
                panic!("Parameter has no possible cc value!")
            }
            ParamType::ZeroTenFloat(_) => ParamType::ZeroOneFloat((v * 10.0).clamp(0.0, 10.0)),
            ParamType::ZeroOneFloat(_) => ParamType::ZeroOneFloat(v.clamp(0.0, 1.0)),
            ParamType::Float32(_) => ParamType::Float32(v * 100.0),
            ParamType::U8(_) => ParamType::U8((v * 127.0).clamp(0.0, 127.0).round() as u8),
        }
    }
    pub fn to_toml_value(&self) -> Value {
        match self {
            ParamType::U8(a) => Value::Integer(*a as i64),
            ParamType::Oscillator(a) | ParamType::Noise(a) | ParamType::String(a) => {
                Value::String(a.to_string())
            }
            ParamType::ZeroOneFloat(a) | ParamType::Float32(a) | ParamType::ZeroTenFloat(a) => {
                Value::Float(*a as f64)
            }
            ParamType::ADSR(array) => Value::Array(
                array
                    .to_vec()
                    .into_iter()
                    .map(|x| Value::Float(x as f64))
                    .collect(),
            ),
        }
    }

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
            ParamType::Float32(v) => Ok((*v / 100.0).clamp(0.0, 1.0)), // scale to 0.0-1.0
            ParamType::ZeroTenFloat(v) => Ok((v.clamp(0.0, 10.0) / 10.0)),
            ParamType::Noise(_) => Err(anyhow!("ParamType::Noise has no numeric value!")),
            ParamType::String(_) => Err(anyhow!("ParamType::String has no numeric value!")),
        }
    }

    pub fn as_f32(&self) -> Result<f32, anyhow::Error> {
        match &self {
            ParamType::U8(v) => Ok(*v as f32),
            ParamType::Oscillator(_) => Err(anyhow!("ParamType::Oscillator has no numeric value!")),
            ParamType::ADSR(_) => Err(anyhow!("ParamType::ADSR has no numeric value!")),
            ParamType::ZeroOneFloat(v) => Ok(v.clamp(0.0, 1.0)),
            ParamType::Float32(v) => Ok(*v),
            ParamType::ZeroTenFloat(v) => Ok(*v),
            ParamType::Noise(_) => Err(anyhow!("ParamType::Noise has no numeric value!")),
            ParamType::String(_) => Err(anyhow!("ParamType::String has no numeric value!")),
        }
    }
    pub fn as_oscillator_type(&self) -> Result<OscillatorType, &'static str> {
        match self {
            ParamType::Oscillator(s) => OscillatorType::from_str(s),
            _ => Err("parameter is not a string, cannot convert to oscillator type"),
        }
    }

    pub fn as_noise_type(&self) -> Result<NoiseType, &'static str> {
        match self {
            ParamType::Oscillator(s) => NoiseType::from_str(s),
            _ => Err("parameter is not a string, cannot convert to noise type"),
        }
    }

    pub fn as_string(&self) -> Result<&str, &'static str> {
        match self {
            ParamType::Oscillator(s) | ParamType::String(s) | ParamType::Noise(s) => Ok(s),
            _ => Err("parameter is not a string, cannot convert to string"),
        }
    }
}

impl std::fmt::Display for ParamType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParamType::U8(v) => write!(f, "{}", v),
            ParamType::Oscillator(s) => write!(f, "{}", s),
            ParamType::ZeroOneFloat(v) | ParamType::Float32(v) | ParamType::ZeroTenFloat(v) => {
                write!(f, "{}", v)
            }
            ParamType::ADSR(v) => write!(f, "{:?}", v),
            ParamType::Noise(v) | ParamType::String(v) => write!(f, "{:?}", v),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CcParam {
    pub value: ParamType,
    pub cc_norm_index: usize,
    pub name: &'static str,
    pub description: Option<&'static str>,
}

#[derive(Debug, Clone)]
pub struct NonCcParam {
    pub value: ParamType,
    pub name: &'static str,
    pub description: Option<&'static str>,
}

impl CcParam {
    fn normalized_to_idx(&self) -> Option<usize> {
        match self.cc_norm_index {
            0 => None,
            _ => Some(self.cc_norm_index - 1),
        }
    }
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
                //println!("param: {} with index: {}", cc_param.name, cc_param.cc_norm_index);
                if cc_param.cc_norm_index >= 1 {
                    cc_array[cc_param.cc_norm_index - 1] =
                        cc_param.value.as_zero_to_one_f32().unwrap()
                }
            }
        }
        cc_array
    }
}

impl Parameterized {
    pub fn param_from_norm_index(&self, idx: usize) -> Option<&CcParam> {
        if let Some(cc_params) = self.cc_params.as_ref() {
            return cc_params.iter().find(|p| p.cc_norm_index == idx);
        }
        None
    }

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

    /// Returns a static ADSR envelope in a `Box` based on internal parameters.
    pub fn boxed_static_adsr(
        &self,
        adsr_param_name: &str,
        state: &SharedMidiState,
    ) -> Box<dyn AudioUnit> {
        let gate = state.gate_var();
        if let Ok(param) = self.get_non_cc_param(adsr_param_name) {
            Box::new(
                gate >> adsr_live(
                    param.value.as_array().unwrap()[0],
                    param.value.as_array().unwrap()[1],
                    param.value.as_array().unwrap()[2],
                    param.value.as_array().unwrap()[3],
                ),
            )
        } else {
            // default:
            Box::new(gate >> adsr_live(0.001, 0.001, 0.95, 0.3))
        }
    }

    pub fn boxed_cc_adsr(&self, adsr: CcADSR, state: &SharedMidiState) -> Box<dyn AudioUnit> {
        let gate = state.gate_var();
        Box::new(gate >> adsr)
    }

    pub(crate) fn get_cc_adsr_params(
        &self,
        attack: &str,
        decay: &str,
        sustain: &str,
        release: &str,
        state: &SharedMidiState,
    ) -> (CcNode, CcNode, CcNode, CcNode) {
        let attack = self.sound_cc_or_map(attack, state, |x| x.value.as_f32().unwrap() / 10.0);
        let decay = self.sound_cc_or_map(decay, state, |x| x.value.as_f32().unwrap() / 10.0);
        let sustain = self.sound_cc_or_default(sustain, state);
        let release = self.sound_cc_or_map(release, state, |x| x.value.as_f32().unwrap() / 10.0);
        (attack, decay, sustain, release)
    }

    fn get_cc_param(&self, name: &str) -> anyhow::Result<&CcParam> {
        if let Some(vec) = &self.cc_params {
            for i in vec.iter() {
                if i.name == name {
                    return Ok(i);
                }
            }
        }
        Err(anyhow!(format!(
            "CC Parameter {} value not found for {}",
            name, self.name
        )))
    }

    pub fn to_toml_values(&self) -> HashMap<String, Value> {
        let mut new_map: HashMap<String, Value> = HashMap::new();
        if let Some(params) = self.cc_params.as_ref() {
            for def in params.iter() {
                new_map.insert(def.name.to_string(), def.value.to_toml_value());
            }
        };
        if let Some(non_cc_params) = self.non_cc_params.as_ref() {
            for def in non_cc_params.iter() {
                new_map.insert(def.name.to_string(), def.value.to_toml_value());
            }
        }
        new_map
    }

    pub fn import_cc_state(&mut self, cc_array: &CcArray) {
        if let Some(cow_cc) = self.cc_params.as_mut() {
            let params = cow_cc.to_mut();
            for def in params.iter_mut() {
                if let Some(idx) = def.normalized_to_idx() {
                    def.value = def.value.with_cc_to_f32(cc_array[idx]);
                }
            }
        }
    }

    pub fn sound_cc_or_default(&self, name: &str, shared: &SharedMidiState) -> CcNode {
        shared.sound_cc_or_default(&self.get_cc_param(name).unwrap())
    }

    pub fn fx_cc_or_default(&self, name: &str, shared: &SharedMidiState) -> CcNode {
        shared.fx_cc_or_default(&self.get_cc_param(name).unwrap())
    }

    pub fn fx_cc_or_map<F>(&self, name: &str, shared: &SharedMidiState, func: F) -> CcNode
    where
        F: FnOnce(&CcParam) -> f32,
    {
        let post_fn = func(&self.get_cc_param(name).unwrap());
        shared.fx_cc_or(&self.get_cc_param(name).unwrap(), post_fn)
    }

    pub fn sound_cc_or_map<F>(&self, name: &str, shared: &SharedMidiState, func: F) -> CcNode
    where
        F: FnOnce(&CcParam) -> f32,
    {
        let post_fn = func(&self.get_cc_param(name).unwrap());
        shared.sound_cc_or(&self.get_cc_param(name).unwrap(), post_fn)
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
    pub fn get_node_type(&self, name: &str) -> anyhow::Result<OscillatorType> {
        let param = self
            .get_non_cc_param(name)
            .map_err(|_| anyhow::anyhow!("parameter not found"))?;
        param
            .value
            .as_oscillator_type()
            .map_err(|e| anyhow::anyhow!(e))
    }

    pub fn get_noise_node_type(&self, name: &str) -> anyhow::Result<NoiseType> {
        let param = self
            .get_non_cc_param(name)
            .map_err(|_| anyhow::anyhow!("parameter not found"))?;
        param.value.as_noise_type().map_err(|e| anyhow::anyhow!(e))
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
                ParamType::ZeroOneFloat(v) | ParamType::Float32(v) | ParamType::ZeroTenFloat(v) => {
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
                ParamType::Oscillator(s) | ParamType::Noise(s) | ParamType::String(s) => {
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

/// Use costume mapping from the toml file's program overrides.
pub fn apply_toml_mapping(params: &mut Parameterized, toml_mapping: &HashMap<String, Value>) {
    if let Some(ref mut cc_params) = params.cc_params {
        let params_mut = cc_params.to_mut();
        for param in params_mut.iter_mut() {
            if let Some(val) = toml_mapping.get(param.name).and_then(|v| v.as_integer()) {
                param.cc_norm_index = val as usize
            }
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
    Lorenz,
    Hammond,
    OrganWave,
    WaveTable(PathBuf),
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
            "lorenz" => Ok(OscillatorType::Lorenz),
            "hammond" => Ok(OscillatorType::Hammond),
            "organ_wave" => Ok(OscillatorType::OrganWave),
            "organ" => Ok(OscillatorType::OrganWave),
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

impl ParamNode<U1, U1> for OscillatorType {
    fn get_node(&self) -> An<Unit<U1, U1>> {
        to_mono_unit(self.as_audiounit())
    }

    fn as_audiounit(&self) -> Box<dyn AudioUnit> {
        match self {
            OscillatorType::Saw => Box::new(poly_saw()),
            OscillatorType::Triangle => Box::new(triangle()),
            OscillatorType::Sine => Box::new(sine()),
            OscillatorType::Pulse => Box::new(poly_square()),
            OscillatorType::Square => Box::new(poly_square()),
            OscillatorType::None => Box::new(sine() * 0.0),
            OscillatorType::Lorenz => Box::new(lorenz()),
            OscillatorType::Hammond => Box::new(hammond()),
            OscillatorType::OrganWave => Box::new(organ()),
            OscillatorType::WaveTable(s) => Self::wavetable_synth_from_path(s),
        }
    }
}

impl OscillatorType {
    fn wavetable_synth_from_path(path: &PathBuf) -> Box<An<WaveSynth<U1>>> {
        let mut wav_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        wav_path.push(path);
        let wave = Wave::load(wav_path).expect("Failed to load WAV file for wavetable synth!");
        let wavetable = Wavetable::from_wave(20.0, 20000.0, 12.0, wave.channel(0));
        let synth = WaveSynth::new(Arc::new(wavetable));
        Box::new(An(synth))
    }

    pub fn get_pwm_node(&self) -> PwNode {
        // nullify the second value for osc that don't support pulse width
        let pw_sinker = (pass() | pass() * 0.0) >> join::<U2>();
        match self {
            OscillatorType::Saw
            | OscillatorType::Triangle
            | OscillatorType::Sine
            | OscillatorType::Square
            | OscillatorType::Lorenz
            | OscillatorType::Hammond
            | OscillatorType::OrganWave => {
                stereo_to_mono_unit(Box::new(pw_sinker >> self.get_node()))
            }
            OscillatorType::Pulse => stereo_to_mono_unit(Box::new(pulse())),
            OscillatorType::None => stereo_to_mono_unit(Box::new(pw_sinker >> sine() * 0.0)),
            OscillatorType::WaveTable(s) => {
                stereo_to_mono_unit(Box::new(pw_sinker >> *Self::wavetable_synth_from_path(&s)))
            }
        }
    }
}
pub trait ParamNode<N, M>
where
    N: ArrayLength + Send + Sync,
    M: ArrayLength + Send + Sync,
{
    fn get_node(&self) -> An<Unit<N, M>>;

    fn as_audiounit(&self) -> Box<dyn AudioUnit>;
}

impl ParamNode<U0, U1> for NoiseType {
    fn get_node(&self) -> An<Unit<U0, U1>> {
        to_zero_mono_unit(self.as_audiounit())
    }

    fn as_audiounit(&self) -> Box<dyn AudioUnit> {
        match self {
            NoiseType::White => Box::new(white()),
            NoiseType::Brown => Box::new(brown()),
            NoiseType::Pink => Box::new(pink()),
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
            "noise" => Ok(NoiseType::White),
            _ => Err("Unrecognized noise type"),
        }
    }
}
