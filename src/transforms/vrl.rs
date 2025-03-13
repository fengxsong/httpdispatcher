use crate::component::{Component, Event, Transform};
use crate::config::RemapTransformConfig;
use anyhow::{anyhow, Error, Result};
use async_trait::async_trait;
use std::any::Any;
use std::collections::BTreeMap;
use std::sync::Arc;
use tracing::debug;
use vrl::compiler::CompilationResult;
use vrl::prelude::NotNan;
use vrl::value::Value as VrlValue;
use vrl::{
    compiler::{state::RuntimeState, Context, TargetValue, TimeZone},
    value,
    value::Secrets,
};

pub struct VrlTransform {
    name: String,
    inputs: Vec<String>,
    result: Arc<CompilationResult>,
}

impl VrlTransform {
    pub fn new(name: String, config: RemapTransformConfig) -> Result<Self, Error> {
        let fns = vrl::stdlib::all();
        let result = vrl::compiler::compile(&config.source, &fns)
            .map_err(|e| anyhow!("Failed to compile VRL transform: {:?}", e))?;

        Ok(Self {
            name: name,
            inputs: config.inputs.clone(),
            result: Arc::new(result),
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

    fn convert_from_vrl_value(value: VrlValue) -> serde_json::Value {
        match value {
            VrlValue::Null => serde_json::Value::Null,
            VrlValue::Boolean(b) => serde_json::Value::Bool(b),
            VrlValue::Integer(i) => serde_json::Value::Number(i.into()),
            VrlValue::Float(f) => {
                if let Some(num) = serde_json::Number::from_f64(f.into()) {
                    serde_json::Value::Number(num)
                } else {
                    serde_json::Value::Null
                }
            }
            VrlValue::Bytes(b) => {
                serde_json::Value::String(String::from_utf8_lossy(&b).into_owned())
            }
            VrlValue::Array(a) => {
                serde_json::Value::Array(a.into_iter().map(Self::convert_from_vrl_value).collect())
            }
            VrlValue::Timestamp(t) => serde_json::Value::Number(t.timestamp().into()),
            VrlValue::Object(o) => {
                let mut map = serde_json::Map::new();
                for (k, v) in o {
                    map.insert(k.to_string(), Self::convert_from_vrl_value(v));
                }
                serde_json::Value::Object(map)
            }
            _ => serde_json::Value::Null,
        }
    }
}

unsafe impl Send for VrlTransform {}
unsafe impl Sync for VrlTransform {}

impl Component for VrlTransform {
    fn name(&self) -> &str {
        &self.name
    }

    fn type_(&self) -> &str {
        "remap"
    }

    fn inputs(&self) -> &[String] {
        &self.inputs
    }
}

#[async_trait]
impl Transform for VrlTransform {
    async fn transform(&self, event: &Event) -> Result<Event, Error> {
        let mut target = TargetValue {
            value: VrlTransform::convert_to_vrl_value(&event.data),
            metadata: value!(event.clone().metadata),
            secrets: Secrets::default(),
        };

        let mut state = RuntimeState::default();
        let timezone = TimeZone::default();

        // A context bundles all the info necessary for the runtime to resolve a value.
        let mut ctx = Context::new(&mut target, &mut state, &timezone);

        // This executes the VRL program, making any modifications to the target, and returning a result.
        let result = self.result.program.resolve(&mut ctx)?;

        debug!("VRL transform result: {}", result.to_string());
        let transformed_data = Self::convert_from_vrl_value(result);
        Ok(Event {
            id: event.clone().id,
            data: transformed_data,
            metadata: event.clone().metadata,
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
