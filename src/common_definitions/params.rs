#[derive(Debug, Clone)]
pub enum ParamType {
    Float(f32),
    Int(usize),
    String(String),
    ZeroToOneFloat(f32),
}

impl ParamType {
    pub fn get_f32(&self) -> Option<f32> {
        match self {
            ParamType::Float(v) => Some(*v),
            ParamType::Int(v) => Some(*v as f32),
            ParamType::String(_) => None,
            ParamType::ZeroToOneFloat(v) => (Some(*v)),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ParamDefault {
    Float(f32),
    ZeroToOneFloat(f32),
    Int(i64),
    String(&'static str),
}
