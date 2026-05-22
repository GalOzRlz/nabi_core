use crate::SharedMidiState;
use crate::config_builder::TomlEffectSection;
use crate::patch_builder::{KnobGroup, KnobLabel, ParamInfo};
use fundsp::prelude::{U2, multipass};
use fundsp::prelude64::{AudioUnit, Net};
use std::collections::HashMap;
use std::sync::Arc;
use toml::Table;

#[macro_export]
macro_rules! register_effect {
    (
        name: $name:expr,
        params: $params_type:ty,
        factory: $factory_fn:ident,
        cc_params: [ $( ($cc_name:expr, $cc_default_knob:expr) ),* $(,)? ]
    ) => {
        inventory::submit! {
            EffectDef {
                name: $name,
                factory: (|config: &toml::Table,
                           cc_map: &std::collections::HashMap<String, usize>|
                 -> EffectFunc {
                    let params = <$params_type as Parameterized>::from_table(config);
                    $factory_fn(&params, cc_map)
                }) as fn(
                    &toml::Table,
                    &std::collections::HashMap<String, usize>,
                ) -> EffectFunc,
                // Store the function pointer, not the result
                param_info: <$params_type as Parameterized>::param_info as fn() -> &'static [ParamInfo],
                cc_params: &[ $( ($cc_name, $cc_default_knob) ),* ],
            }
        }
    };
}

pub type EffectFunc = Box<dyn Fn(&SharedMidiState) -> Net + Send + Sync + 'static>;

pub type EffectFactory = fn(construction: &Table, knob_map: &HashMap<String, usize>) -> EffectFunc;

pub struct EffectDef {
    pub name: &'static str,
    pub factory: EffectFactory,
    /// Returns parameter metadata when called.
    pub param_info: fn() -> &'static [ParamInfo],
    pub cc_params: &'static [(&'static str, usize)],
}

inventory::collect!(EffectDef);

#[derive(Clone)]
pub struct FxChainFactory {
    pub chain: Arc<Vec<EffectFunc>>,
    pub initial_cc: Vec<f32>, // was CcValuesArray — now dynamic
    pub knob_labels: Vec<KnobLabel>,
}

impl FxChainFactory {
    pub fn build(&mut self, shared_midi_state: &SharedMidiState) -> Net {
        let arc_vec: Arc<Vec<Net>> =
            Arc::new(self.chain.iter().map(|fx| fx(shared_midi_state)).collect());
        connect_node_vec(arc_vec, None)
    }
    pub fn new(
        effects_config: Option<&TomlEffectSection>,
        effect_cc_count: usize, // from GlobalConfig.fx_cc_mapping.len()
    ) -> Self {
        // Build the effect registry once
        let registry: HashMap<&str, &EffectDef> = inventory::iter::<EffectDef>()
            .map(|e| (e.name, e))
            .collect();
        let mut knob_labels = Vec::with_capacity(effect_cc_count);
        // If there are no effects, return an empty chain
        let Some(effects) = effects_config else {
            return FxChainFactory {
                chain: Arc::new(Vec::new()),
                initial_cc: vec![0.0; effect_cc_count.max(1)],
                knob_labels: Vec::new(),
            };
        };
        let mut initial_knobs = vec![0.0_f32; effect_cc_count.max(1)];
        let mut chain = Vec::new();

        for fx_name in &effects.chain {
            let def = registry
                .get(fx_name.as_str())
                .unwrap_or_else(|| panic!("Unknown effect: {}", fx_name));

            // ---- Construction values (raw TOML table, exactly what the factory expects) ----
            let mut construction = toml::Table::new();
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
            for (fx_name, default_knob) in def.cc_params.iter() {
                let mut knob = *default_knob;

                // User override?
                if let Some(m) = user_mappings {
                    if let Some(val) = m.get(*fx_name).and_then(|v| v.as_integer()) {
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
                    label: format!("{}: {}", fx_name, fx_name),
                });
            }
            for (param_name, value) in construction.iter() {
                for (name, index) in knob_map.iter() {
                    if name == param_name {
                        initial_knobs.insert(
                            *index - 1, // cc is 1-..
                            value
                                .as_float()
                                .expect("illegal value for initialization param!")
                                as f32,
                        )
                    }
                }
            }
            // The factory converts `construction` into the proper struct internally
            let closure = (def.factory)(&construction, &knob_map);
            chain.push(closure);
        }
        println!("effects factory: {:?}", effects.chain);
        println!("lables: {:?}", knob_labels);
        FxChainFactory {
            chain: Arc::new(chain),
            initial_cc: initial_knobs,
            knob_labels,
        }
    }
}

pub fn to_stereo(net: Net) -> Net {
    match net.inputs() {
        1 => (net.clone() | net),
        2 => net,
        _ => panic!("only 1 and 2 inputs are supported!"),
    }
}

fn connect_node_vec(node_vec: Arc<Vec<Net>>, starting_net: Option<Net>) -> Net {
    let nodes = (*node_vec).clone();
    let mut net = starting_net.unwrap_or_else(|| Net::wrap(Box::new(multipass::<U2>())));
    for node in nodes {
        net = to_stereo(net) >> node;
    }
    net
}
