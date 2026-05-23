use serde::de::DeserializeOwned;
use toml::Table;

#[derive(Debug, Clone)]
pub enum ParamType {
    Float,
    Int,
    String,
    ZeroToOneFloat,
}

#[derive(Debug, Clone)]
pub enum ParamDefault {
    Float(f32),
    ZeroToOneFloat(f32),
    Int(i64),
    String(&'static str),
}

#[derive(Debug, Clone)]
pub struct ParamInfo {
    pub name: &'static str,
    pub param_type: ParamType,
    pub default: ParamDefault,
    pub description: Option<&'static str>,
}
pub trait Parameterized: Sized {
    fn param_info() -> &'static [ParamInfo];

    fn from_table(table: &Table) -> Self
    where
        Self: DeserializeOwned + Default,
    {
        let value: toml::Value = table.clone().into();
        Self::deserialize(value).unwrap_or_default()
    }
}
