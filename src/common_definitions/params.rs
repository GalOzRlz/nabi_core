use anyhow::anyhow;
use std::borrow::Cow;
use toml::Table;

#[derive(Debug, Clone)]
pub enum ParamType {
    Float(f32),
    Int(usize),
    String(String),
    ZeroToOneFloat(f32),
}
impl ParamType {
    pub fn as_f32(&self) -> Option<f32> {
        match self {
            ParamType::Float(v) => Some(*v),
            ParamType::Int(v) => Some(*v as f32),
            ParamType::String(_) => None,
            ParamType::ZeroToOneFloat(v) => (Some(v.clamp(0.0, 1.0))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CcParam {
    pub default: ParamType,
    pub cc_index: usize,
    pub name: &'static str,
}

#[derive(Debug, Clone)]
pub struct NonCcParam {
    pub value: ParamType,
    pub name: &'static str,
}
#[derive(Clone)]
pub(crate) struct Parameterized {
    pub(crate) name: &'static str,
    pub(crate) cc_params: Option<Cow<'static, [CcParam]>>,
    pub(crate) non_cc_params: Option<Cow<'static, [NonCcParam]>>,
}
impl Parameterized {
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

    pub fn get_non_cc_param(&self, name: &str) -> anyhow::Result<&NonCcParam> {
        if let Some(vec) = &self.non_cc_params {
            for i in vec.iter() {
                if i.name == name {
                    return Ok(i);
                }
            }
        }
        Err(anyhow!(format!("Non-CC-Parameter {} not found", name)))
    }
}

pub trait ValuedParam {
    fn get_mut(&mut self) -> &mut ParamType;
}

impl ValuedParam for CcParam {
    fn get_mut(&mut self) -> &mut ParamType {
        &mut self.default
    }
}
impl ValuedParam for NonCcParam {
    fn get_mut(&mut self) -> &mut ParamType {
        &mut self.value
    }
}

pub fn apply_toml_overrides<T>(params: &mut [T], fx_name: &str, toml_overrides: &Table)
where
    T: ValuedParam,
{
    for param in params {
        if let Some(toml_value) = toml_overrides.get(fx_name) {
            match param.get_mut() {
                ParamType::Float(v) | ParamType::ZeroToOneFloat(v) => {
                    if let Some(num) = toml_value.as_float() {
                        *v = num as f32;
                    }
                }
                ParamType::Int(v) => {
                    if let Some(num) = toml_value.as_integer() {
                        *v = num as usize;
                    } else if let Some(num) = toml_value.as_float() {
                        *v = num as usize;
                    }
                }
                ParamType::String(s) => {
                    if let Some(str_val) = toml_value.as_str() {
                        *s = str_val.to_string();
                    }
                }
            }
        }
    }
}
