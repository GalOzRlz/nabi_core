use crate::SharedMidiState;
use crate::common_definitions::params::{Parameterized, apply_toml_overrides};
use crate::config_builder::{MAX_KNOBS_PER_GROUP, TomlEffectSection};
use crate::effects::helpers::to_stereo;
use crate::effects::master_fx::EFFECTS;
use fundsp::prelude64::{Net, NodeId};
use std::collections::HashMap;
use std::sync::Arc;
use toml::Table;

pub type EffectFunc = Box<dyn Fn(&SharedMidiState) -> Net + Send + Sync + 'static>;

pub type EffectFactory = fn(construction: &Table, knob_map: &HashMap<String, usize>) -> EffectFunc;

#[derive(Clone)]
pub struct EffectDef {
    pub factory: fn(Parameterized) -> EffectFunc,
    pub params: Parameterized,
}

#[derive(Clone)]
pub struct FxChainFactory {
    pub chain: Arc<Vec<EffectFunc>>,
    pub node_ids: Option<Vec<NodeId>>,
    pub definitions: Option<Vec<Parameterized>>,
    pub fx_names: Option<Vec<String>>,
}

impl FxChainFactory {
    pub fn get_initial_cc(&self) -> [f32; MAX_KNOBS_PER_GROUP] {
        let mut cc_array = [0_f32; MAX_KNOBS_PER_GROUP];
        if let Some(definitions) = &self.definitions {
            for params in definitions {
                for cc_params_cow in &params.cc_params {
                    for cc_param in cc_params_cow.iter() {
                        cc_array[cc_param.cc_index] = cc_param.default.as_f32().unwrap()
                    }
                }
            }
        }
        println!("initial cc array: {:?}", cc_array);
        cc_array
    }

    pub fn connect_node_vec(&mut self, node_vec: Arc<Vec<Net>>) -> Net {
        let mut nodeid_vec: Vec<NodeId> = Vec::with_capacity(node_vec.len());
        let nodes = (*node_vec).clone();
        let mut net = Net::new(2, 2);
        for node in nodes {
            let id = net.chain(Box::new(to_stereo(node)));
            nodeid_vec.push(id)
        }
        self.node_ids = Some(nodeid_vec);
        net
    }

    pub fn build(&mut self, shared_midi_state: &SharedMidiState) -> Net {
        println!("initial cc: {:?}", self.get_initial_cc());
        let arc_vec: Arc<Vec<Net>> =
            Arc::new(self.chain.iter().map(|fx| fx(shared_midi_state)).collect());
        self.connect_node_vec(arc_vec)
    }
    pub fn new(effects_config: Option<&TomlEffectSection>) -> Self {
        let registry: HashMap<&str, &EffectDef> =
            EFFECTS.iter().map(|e| (e.params.name.clone(), e)).collect();
        let Some(effects) = effects_config else {
            return FxChainFactory {
                chain: Arc::new(Vec::new()),
                node_ids: None,
                fx_names: None,
                definitions: None,
            };
        };
        let mut definitions = Vec::new();
        let mut fx_names = Vec::new();
        let mut chain = Vec::new();
        for fx_name in &effects.chain {
            let mut def = registry
                .get(fx_name.as_str())
                .unwrap_or_else(|| panic!("Unknown effect: {}", fx_name));
            let mut runtime_params = def.params.clone();
            // ---- Construction values (raw TOML table, exactly what the factory expects) ----
            let mut toml_overrides = Table::new();
            if let Some(eff_cfg) = effects
                .extras
                .get(fx_name.as_str())
                .and_then(|v| v.as_table())
            {
                for (k, v) in eff_cfg {
                    if k != "mapping" {
                        toml_overrides.insert(k.clone(), v.clone());
                    }
                }
            }

            // cc mapping
            let user_mappings: Option<&Table> = effects
                .extras
                .get(fx_name.as_str())
                .and_then(|v| v.get("mapping"))
                .and_then(|v| v.as_table());

            if let Some(ref mut cc_params) = runtime_params.cc_params {
                let params_mut = cc_params.to_mut(); // &mut [CcParam], call once
                for param in params_mut.iter_mut() {
                    if let Some(m) = &user_mappings {
                        if let Some(val) = m.get(fx_name).and_then(|v| v.as_integer()) {
                            param.cc_index = val as usize;
                        }
                    }
                }
                apply_toml_overrides(cc_params.to_mut(), fx_name, &toml_overrides);
            }
            if let Some(ref mut non_cc_params) = runtime_params.non_cc_params {
                apply_toml_overrides(non_cc_params.to_mut(), fx_name, &toml_overrides);
            }

            definitions.push(runtime_params.clone());
            let closure = (def.factory)(runtime_params);
            chain.push(closure);
            fx_names.push(fx_name.to_string());
        }
        FxChainFactory {
            chain: Arc::new(chain),
            node_ids: None,
            fx_names: Some(fx_names),
            definitions: Some(definitions),
        }
    }
}
