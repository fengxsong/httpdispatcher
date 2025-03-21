use crate::component::{Component, Event, Transform};
use crate::config::RouteTransformConfig;
use anyhow::{anyhow, Error, Result};
use async_trait::async_trait;
use dashmap::DashMap;
use std::any::Any;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::warn;
use vrl::compiler::CompilationResult;
use vrl::prelude::NotNan;
use vrl::value::Value as VrlValue;
use vrl::{
    compiler::{state::RuntimeState, Context, TargetValue, TimeZone},
    value,
    value::Secrets,
};

pub struct RouteTransform {
    name: String,
    reroute_unmatched: bool,
    inputs: Vec<String>,
    routes: HashMap<String, Arc<CompilationResult>>,
    output_channels: DashMap<String, broadcast::Sender<Event>>,
}

impl RouteTransform {
    pub fn new(
        name: String,
        config: RouteTransformConfig,
        output_channels: DashMap<String, broadcast::Sender<Event>>,
    ) -> Result<Self, Error> {
        let fns = vrl::stdlib::all();
        let mut routes = HashMap::new();

        for (route_name, rule) in &config.routes {
            let result = vrl::compiler::compile(&rule.source, &fns).map_err(|e| {
                anyhow!(
                    "Failed to compile VRL condition for route {}: {:?}",
                    route_name,
                    e
                )
            })?;
            routes.insert(route_name.clone(), Arc::new(result));
        }

        Ok(Self {
            name,
            reroute_unmatched: config.reroute_unmatched,
            inputs: config.inputs.clone(),
            routes,
            output_channels,
        })
    }

    fn convert_to_vrl_value(value: &serde_json::Value) -> VrlValue {
        match value {
            serde_json::Value::Null => VrlValue::Null,
            serde_json::Value::Bool(b) => VrlValue::Boolean(*b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    VrlValue::Integer(i)
                } else if let Some(f) = n.as_f64() {
                    VrlValue::Float(NotNan::new(f).unwrap())
                } else {
                    VrlValue::Null
                }
            }
            serde_json::Value::String(s) => VrlValue::Bytes(s.clone().into()),
            serde_json::Value::Array(a) => {
                VrlValue::Array(a.iter().map(Self::convert_to_vrl_value).collect())
            }
            serde_json::Value::Object(o) => {
                let mut map = BTreeMap::new();
                for (k, v) in o {
                    map.insert(k.clone().into(), Self::convert_to_vrl_value(v));
                }
                VrlValue::Object(map)
            }
        }
    }
}

unsafe impl Send for RouteTransform {}
unsafe impl Sync for RouteTransform {}

impl Component for RouteTransform {
    fn name(&self) -> &str {
        &self.name
    }

    fn type_(&self) -> &str {
        "route"
    }

    fn inputs(&self) -> &[String] {
        &self.inputs
    }
}

#[async_trait]
impl Transform for RouteTransform {
    async fn transform(&self, event: &Event) -> Result<Event, Error> {
        let mut routed = false;

        let event_value = RouteTransform::convert_to_vrl_value(&event.data);
        let event_metadata = value!(event.clone().metadata);

        let mut state = RuntimeState::default();
        let timezone = TimeZone::default();

        for (route_name, compiled_condition) in &self.routes {
            let mut target = TargetValue {
                value: event_value.clone(),
                metadata: event_metadata.clone(),
                secrets: Secrets::default(),
            };

            let mut ctx = Context::new(&mut target, &mut state, &timezone);

            match compiled_condition.program.resolve(&mut ctx) {
                Ok(result) => {
                    if let VrlValue::Boolean(true) = result {
                        let output_key = format!("{}.{}", self.name, route_name);

                        if let Some(output_channel) = self.output_channels.get(&output_key) {
                            if let Err(e) = output_channel.send(event.clone()) {
                                warn!("Failed to send event to route {}: {}", output_key, e);
                            } else {
                                routed = true;
                            }
                        } else {
                            unreachable!("Output channel not found for route: {}", output_key);
                        }
                    }
                }
                Err(e) => {
                    warn!("Error evaluating route condition for {}: {}", route_name, e);
                }
            }
        }

        if !routed && self.reroute_unmatched {
            let default_key = format!("{}._unmatched", self.name);
            if let Some(default_channel) = self.output_channels.get(&default_key) {
                if let Err(e) = default_channel.send(event.clone()) {
                    warn!("Failed to send unmatched event to default route: {}", e);
                }
            } else {
                unreachable!("Default output channel not found: {}", default_key);
            }
        }

        Ok(event.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
