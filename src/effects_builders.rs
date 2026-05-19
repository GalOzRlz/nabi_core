use crate::SharedMidiState;
use crate::config_builder::{MAX_KNOBS_PER_GROUP, TomlEffectSection};
use crate::effects::to_net;
use crate::patch_builder::{KnobGroup, KnobLabel};
use fundsp::prelude::{U2, multipass};
use fundsp::prelude64::{AudioUnit, Net};
use std::collections::HashMap;
use std::sync::Arc;
use toml::Table;

#[macro_export]
macro_rules! register_effect {
    (
        name: $name:expr,
        factory: $factory_fn:ident,
        construction_params: [ $( ($c_name:ident, $c_default:expr) ),* $(,)? ],
        cc_params: [ $( ($cc_name:expr, $cc_default_knob:expr, $cc_default_val:expr) ),* $(,)? ]
    ) => {
        paste::paste! {
            pub struct [<$name:camel Params>] {
                $( pub $c_name: f64, )*
            }

            impl [<$name:camel Params>] {
                fn from_table(table: &toml::Table) -> Self {
                    Self {
                        $(
                            $c_name: table.get(stringify!($c_name))
                                .and_then(|v| v.as_float())
                                .unwrap_or($c_default),
                        )*
                    }
                }
            }

            fn [<__effect_wrapper_ $name:snake>] (
                construction: &toml::Table,
                cc_map: &std::collections::HashMap<String, usize>,
            ) -> $crate::effects_builders::EffectFunc {
                let params = [<$name:camel Params>]::from_table(construction);
                $factory_fn(&params, cc_map)
            }

            inventory::submit! {
                $crate::effects_builders::EffectDef {
                    name: $name,
                    factory: [<__effect_wrapper_ $name:snake>] as fn(
                        &toml::Table,
                        &std::collections::HashMap<String, usize>,
                    ) -> $crate::effects_builders::EffectFunc,
                    construction_defaults: &[ $( (stringify!($c_name), $c_default) ),* ],
                    cc_params: &[ $( ($cc_name, $cc_default_knob, $cc_default_val) ),* ],
                }
            }
        }
    };
}

pub type EffectFunc = Box<dyn Fn(&SharedMidiState) -> Net + Send + Sync + 'static>;

pub type EffectFactory = fn(construction: &Table, knob_map: &HashMap<String, usize>) -> EffectFunc;

pub struct EffectDef {
    pub name: &'static str,
    pub factory: EffectFactory,
    pub construction_defaults: &'static [(&'static str, f64)], // ← plain f64
    pub cc_params: &'static [(&'static str, usize, f32)],
}

inventory::collect!(EffectDef);

#[derive(Clone)]
pub struct PatchFxChain {
    pub chain: Arc<Vec<EffectFunc>>,
    pub initial_cc: Vec<f32>, // was CcValuesArray — now dynamic
    pub knob_labels: Vec<KnobLabel>,
    pub net: Net,
}

impl PatchFxChain {
    pub fn assemble_net(&mut self, shared_midi_state: &SharedMidiState) {
        let arc_vec: Arc<Vec<Net>> =
            Arc::new(self.chain.iter().map(|fx| fx(shared_midi_state)).collect());
        self.net = connect_node_vec(arc_vec, None)
    }
    pub fn new(
        effects: Option<&TomlEffectSection>,
        registry: &HashMap<&str, &EffectDef>,
        effect_knob_count: usize, // from GlobalConfig.effect_knob_ccs.len()
    ) -> Self {
        let mut chain = Vec::new();
        // Dynamic initial knob array – sized to the user's effect knobs
        let mut initial_knobs = vec![0.0_f32; effect_knob_count.max(1)];
        let mut knob_labels = Vec::new();
        let num_knobs = effect_knob_count; // for clamping

        if let Some(effects) = effects {
            for eff_name in &effects.chain {
                let def = registry
                    .get(eff_name.as_str())
                    .unwrap_or_else(|| panic!("Unknown effect: {}", eff_name));

                // ---- Construction values ----
                let mut construction = toml::Table::new();
                // start with defaults from the effect definition
                for (k, v) in def.construction_defaults.iter() {
                    construction.insert(k.to_string(), toml::Value::from(*v));
                }
                // overrides from TOML (excluding "mapping")
                if let Some(eff_cfg) = effects
                    .extras
                    .get(eff_name.as_str())
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
                // User‑specified mappings from TOML
                let user_mappings: Option<&toml::Table> = effects
                    .extras
                    .get(eff_name.as_str())
                    .and_then(|v| v.get("mapping"))
                    .and_then(|v| v.as_table());

                for (param_name, default_knob, default_val) in def.cc_params.iter() {
                    let mut knob = *default_knob;

                    // user override?
                    if let Some(m) = user_mappings {
                        if let Some(val) = m.get(*param_name).and_then(|v| v.as_integer()) {
                            knob = val as usize;
                        }
                    }

                    // clamp to the actual number of available effect knobs
                    if knob < 1 {
                        knob = 1;
                    }
                    if knob > num_knobs {
                        knob = num_knobs;
                    }

                    knob_map.insert(param_name.to_string(), knob);

                    // initial value: config override > default
                    let init_val = if let Some(val) = effects
                        .extras
                        .get(eff_name.as_str())
                        .and_then(|v| v.as_table())
                        .and_then(|t| t.get(*param_name).and_then(|v| v.as_float()))
                    {
                        val as f32 // ✅ use the bound variable
                    } else {
                        *default_val as f32
                    };

                    if knob <= initial_knobs.len() {
                        initial_knobs[knob - 1] = init_val;
                    }

                    // Store label as KnobLabel for the unified system
                    knob_labels.push(KnobLabel {
                        group: KnobGroup::Effect, // always Effect for this chain
                        index: knob,
                        label: format!("{}: {}", eff_name, param_name),
                    });
                }

                // Build the effect closure (factory signature unchanged)
                let closure = (def.factory)(&construction, &knob_map);
                chain.push(closure);
            }
        }
        PatchFxChain {
            chain: Arc::new(chain),
            initial_cc: initial_knobs, // Vec<f32>
            knob_labels,
            net: to_net(multipass::<U2>()), // Vec<KnobLabel> (already changed in struct)
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
