use crate::SharedMidiState;
use crate::config_builder::TomlEffectSection;
use crate::effects::helpers::to_stereo;
use crate::effects::master_fx::EFFECTS;
use crate::effects::params::Parameterized;
use crate::patch_builder::{KnobGroup, KnobLabel};
use fundsp::prelude64::{Net, NodeId};
use std::collections::HashMap;
use std::sync::Arc;
use toml::Table;

#[macro_export]
macro_rules! register_effect {
    (
        name: $name:expr,
        params: $params_type:ty,
        factory: $factory_fn:ident,
    ) => {
        inventory::submit! {
            EffectDef {
                name: $name,
                factory: (|config: Parameterized |
                 -> EffectFunc {
                    let params = $params_type;
                    $factory_fn(&params)
                }) as fn(
                    &toml::Table,
                ) -> EffectFunc,
                config: &[ $( ($cc_name, $cc_default_knob) ),* ],
            }
        }
    };
}

pub type EffectFunc = Box<dyn Fn(&SharedMidiState) -> Net + Send + Sync + 'static>;

pub type EffectFactory = fn(construction: &Table, knob_map: &HashMap<String, usize>) -> EffectFunc;

#[derive(Clone)]
pub struct EffectDef {
    pub factory: fn(Parameterized) -> EffectFunc,
    pub params: Parameterized,
}

inventory::collect!(EffectDef);

#[derive(Clone)]
pub struct FxChainFactory {
    pub chain: Arc<Vec<EffectFunc>>,
    pub initial_cc: Vec<f32>,
    pub knob_labels: Vec<KnobLabel>,
    pub node_ids: Option<Vec<NodeId>>,
}

impl FxChainFactory {
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
        println!("knob lables: {:?}", self.knob_labels);
        println!("initial cc: {:?}", self.initial_cc);
        let arc_vec: Arc<Vec<Net>> =
            Arc::new(self.chain.iter().map(|fx| fx(shared_midi_state)).collect());
        self.connect_node_vec(arc_vec)
    }
    pub fn new(effects_config: Option<&TomlEffectSection>, effect_cc_count: usize) -> Self {
        // Build the effect registry once
        let registry: HashMap<&str, &EffectDef> =
            EFFECTS.iter().map(|e| (e.params.name.clone(), e)).collect();
        let mut knob_labels = Vec::with_capacity(effect_cc_count);
        // If there are no effects, return an empty chain
        let Some(effects) = effects_config else {
            return FxChainFactory {
                chain: Arc::new(Vec::new()),
                initial_cc: vec![0.0; effect_cc_count.max(1)],
                knob_labels: Vec::new(),
                node_ids: None,
            };
        };
        let mut initial_knobs = vec![0.0_f32; effect_cc_count.max(1)];
        let mut chain = Vec::new();

        for fx_name in &effects.chain {
            let def = registry
                .get(fx_name.as_str())
                .unwrap_or_else(|| panic!("Unknown effect: {}", fx_name));

            // ---- Construction values (raw TOML table, exactly what the factory expects) ----
            let mut construction = Table::new();
            if let Some(eff_cfg) = effects
                .extras
                .get(fx_name.as_str())
                .and_then(|v| v.as_table())
            {
                for (k, v) in eff_cfg {
                    if k != "mapping" {
                        construction.insert(k.clone(), v.clone());
                    }
                }
            }

            // ---- CC parameter mappings ----
            let mut knob_map = HashMap::new();
            let user_mappings: Option<&Table> = effects
                .extras
                .get(fx_name.as_str())
                .and_then(|v| v.get("mapping"))
                .and_then(|v| v.as_table());

            // def.cc_params is now &[(name, default_knob)] – no default value
            for param in def.params.cc_params.iter() {
                let mut knob = param.cc_index;

                // User override?
                if let Some(m) = user_mappings {
                    if let Some(val) = m.get(fx_name).and_then(|v| v.as_integer()) {
                        knob = val as usize;
                    }
                }
                // Clamp
                if knob < 1 {
                    knob = 1;
                }
                if knob > effect_cc_count {
                    knob = effect_cc_count;
                }

                knob_map.insert(fx_name.to_string(), knob);

                knob_labels.push(KnobLabel {
                    group: KnobGroup::Effect,
                    index: knob,
                    label: format!("{}", fx_name),
                });
            }
            println!("knob map: {:?}", knob_map);
            for (param_name, value) in construction.iter() {
                for (name, index) in knob_map.iter() {
                    if name == param_name {
                        println!("cc knob {:?} - will get value {:?}", index, *value);
                        initial_knobs[index - 1] = value
                            .as_float()
                            .expect("illegal value for initialization param!")
                            as f32;
                    }
                }
            }
            // The factory converts `construction` into the proper struct internally
            let closure = (def.factory)(def.params.clone());
            chain.push(closure);
        }
        FxChainFactory {
            chain: Arc::new(chain),
            initial_cc: initial_knobs,
            knob_labels,
            node_ids: None,
        }
    }
}
