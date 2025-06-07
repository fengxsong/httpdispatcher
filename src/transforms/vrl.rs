use crate::component::{Component, Event, Transform};
use crate::config::RemapTransformConfig;
use crate::transforms::vrl_utils::{convert_from_vrl_value, convert_to_vrl_value};
use anyhow::{anyhow, Error, Result};
use async_trait::async_trait;
use std::any::Any;
use std::sync::Arc;
use tracing::debug;
use vrl::compiler::CompilationResult;
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
            value: convert_to_vrl_value(&event.data),
            metadata: value!(event.clone().metadata),
            secrets: Secrets::default(),
        };

        let mut state = RuntimeState::default();
        let timezone = TimeZone::default();

        let mut ctx = Context::new(&mut target, &mut state, &timezone);

        let result = self.result.program.resolve(&mut ctx)?;

        debug!("VRL transform result: {}", result.to_string());
        let transformed_data = convert_from_vrl_value(result);
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
