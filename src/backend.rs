use serde::Serialize;
use std::net::TcpListener;
use std::thread;
use tungstenite::accept;
use crate::effects_builders::EffectDef;
use crate::patch_builder::ParamDefault;

#[derive(Serialize)]
pub struct EffectMeta {
    pub name: &'static str,
    pub params: Vec<ParamMeta>,
    pub cc_params: Vec<CcParamMeta>,
}

#[derive(Serialize)]
pub struct ParamMeta {
    pub name: &'static str,
    pub param_type: String,
    pub default: serde_json::Value,
}

#[derive(Serialize)]
pub struct CcParamMeta {
    pub name: &'static str,
    pub default_knob: usize,
}

pub fn get_all_effects_meta() -> Vec<EffectMeta> {
    inventory::iter::<EffectDef>()
        .map(|def| {
            let params = (def.param_info)()
                .iter()
                .map(|p| ParamMeta {
                    name: p.name,
                    param_type: format!("{:?}", p.param_type),
                    default: match &p.default {
                        ParamDefault::Float(v) => serde_json::json!(v),
                        ParamDefault::Int(v) => serde_json::json!(v),
                        ParamDefault::String(v) => serde_json::json!(v),
                    },
                })
                .collect();

            let cc_params = def.cc_params
                .iter()
                .map(|(name, knob)| CcParamMeta { name, default_knob: *knob })
                .collect();

            EffectMeta {
                name: def.name,
                params,
                cc_params,
            }
        })
        .collect()
}


/// Start a WebSocket server on the given port that sends
/// effect & sound metadata to every connecting client.
pub fn start_meta_server(port: u16) {
    thread::spawn(move || {
        let addr = format!("127.0.0.1:{port}");
        let listener = TcpListener::bind(&addr).expect("WebSocket bind failed");
        println!("Meta WebSocket listening on ws://{addr}");

        for stream in listener.incoming() {
            if let Ok(tcp) = stream {
                if let Ok(mut ws) = accept(tcp) {
                    let meta_json = serde_json::to_string(&get_all_effects_meta())
                        .unwrap_or_else(|_| "[]".to_string());

                    // Convert String -> Utf8Bytes with .into()
                    let _ = ws.send(tungstenite::Message::Text(meta_json.into()));
                    let _ = ws.close(None);
                }
            }
        }
    });
}