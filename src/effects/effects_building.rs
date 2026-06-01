use crate::SharedMidiState;
use crate::common::params::{CcInit, Parameterized};
use crate::config_builder::{MAX_KNOBS_PER_GROUP, TomlEffectSection};
use crate::effects::helpers::to_stereo;
use fundsp::prelude64::{Net, NodeId};
use linkme::distributed_slice;
use std::collections::HashMap;
use std::sync::Arc;

#[distributed_slice]
pub static EFFECTS: [EffectDef] = [..];

pub type EffectFunc = Box<dyn Fn(&SharedMidiState) -> Net + Send + Sync + 'static>;

pub type EffectFactory = fn(Arc<Parameterized>) -> EffectFunc;

type EffectsRegistry = HashMap<&'static str, &'static EffectDef>;
pub fn get_effects_registry() -> EffectsRegistry {
    let registry: EffectsRegistry = EFFECTS.iter().map(|e| (e.params.name, e)).collect();
    registry
}

pub fn get_effect_from_registry(fx_name: &str, registry: &EffectsRegistry) -> &'static EffectDef {
    registry
        .get(fx_name)
        .unwrap_or_else(|| panic!("Unknown effect: {}", fx_name))
}

#[derive(Clone)]
pub struct EffectDef {
    pub factory: EffectFactory,
    pub params: Parameterized,
}

#[derive(Clone)]
pub struct FxChainFactory {
    pub chain: Arc<Vec<EffectFunc>>,
    pub node_ids: Option<Vec<NodeId>>,
    pub definitions: Option<Vec<Arc<Parameterized>>>,
    pub fx_names: Option<Vec<String>>,
}

impl CcInit for FxChainFactory {
    fn get_initial_cc(&self) -> [f32; MAX_KNOBS_PER_GROUP] {
        let mut final_cc_array = [0_f32; MAX_KNOBS_PER_GROUP];
        if let Some(definitions) = &self.definitions {
            let mut cc_summation = Vec::with_capacity(definitions.len());
            for def in definitions {
                cc_summation.push(def.get_initial_cc())
            }
            for (param) in cc_summation.iter() {
                for (idx, val) in param.iter().enumerate() {
                    final_cc_array[idx] = *val;
                }
            }
        }
        final_cc_array
    }
}

impl FxChainFactory {
    pub fn connect_node_vec(&mut self, node_vec: Arc<Vec<Net>>) -> Net {
        let mut nodeid_vec: Vec<NodeId> = Vec::with_capacity(node_vec.len());
        let mut net = Net::new(2, 2);
        for node in node_vec.iter() {
            let id = net.chain(Box::new(to_stereo(node.clone())));
            nodeid_vec.push(id)
        }
        self.node_ids = Some(nodeid_vec);
        net
    }
    /// Rebuilds the chain and connects its Net based off of the struct definitions
    pub fn reassembled_chain(&mut self, shared_midi_state: &SharedMidiState) -> Net {
        let registry = get_effects_registry();
        let mut chain = Vec::new();
        for (idx, effect) in self.fx_names.as_ref().unwrap().iter().enumerate() {
            let params_arc = self.definitions.as_ref().unwrap()[idx].clone();
            let factory = get_effect_from_registry(effect, &registry).factory;
            let closure = (factory)(params_arc);
            chain.push(closure);
        }
        self.chain = Arc::new(chain);
        self.build_chain(shared_midi_state)
    }

    /// Builds and connects the nets of the existing chain
    pub fn build_chain(&mut self, shared_midi_state: &SharedMidiState) -> Net {
        let arc_vec: Arc<Vec<Net>> =
            Arc::new(self.chain.iter().map(|fx| fx(shared_midi_state)).collect());
        self.connect_node_vec(arc_vec)
    }
    pub fn new() -> Self {
        FxChainFactory {
            chain: Arc::new(Vec::new()),
            node_ids: None,
            fx_names: None,
            definitions: None,
        }
    }

    pub fn process_config(&self, config: Option<&TomlEffectSection>) -> Self {
        let registry = get_effects_registry();
        let Some(effects_toml_config) = config else {
            return Self::new();
        };
        let mut definitions = Vec::new();
        let mut fx_names = Vec::new();
        let mut chain = Vec::new();
        if let Some(fx_chain) = &effects_toml_config.chain {
            for fx_name in fx_chain.iter() {
                let entry = get_effect_from_registry(fx_name, &registry);
                let mut params = entry.params.clone();
                if let Some(configs) = &effects_toml_config.configs {
                    if let Some(toml_params) = configs.get(fx_name) {
                        params.apply_toml_overrides(toml_params);
                    }
                }
                let runtime_arc = Arc::new(params);
                let closure = (entry.factory)(runtime_arc.clone());
                chain.push(closure);
                fx_names.push(fx_name.to_string());
                definitions.push(runtime_arc);
            }
        }
        FxChainFactory {
            chain: Arc::new(chain),
            node_ids: None,
            fx_names: Some(fx_names),
            definitions: Some(definitions),
        }
    }
}
