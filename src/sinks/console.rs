use crate::component::{Component, Event, Sink};
use anyhow::{Error, Result};
use async_trait::async_trait;
use serde_json::{json, to_string_pretty};
use std::any::Any;
use tera::{Context, Tera};
use tracing::info;

use super::render::load_template;

#[derive(Debug)]
pub struct ConsoleSink {
    name: String,
    format: OutputFormat,
    template: Option<Tera>,
}

#[derive(Debug, Clone)]
pub enum OutputFormat {
    Text { template: Option<String> },
    Json,
}

impl ConsoleSink {
    pub fn new(name: String, format: OutputFormat) -> Result<Self, Error> {
        let mut template = None;
        if let OutputFormat::Text {
            template: Some(tpl),
        } = &format
        {
            let content = load_template(tpl)?;
            let mut tera = Tera::default();
            tera.add_raw_template(&name, &content)?;
            template = Some(tera);
        }

        Ok(Self {
            name,
            format,
            template,
        })
    }
}

impl Component for ConsoleSink {
    fn name(&self) -> &str {
        &self.name
    }

    fn type_(&self) -> &str {
        "console"
    }

    fn inputs(&self) -> &[String] {
        &[]
    }
}

#[async_trait]
impl Sink for ConsoleSink {
    async fn process(&mut self, event: &Event) -> Result<(), Error> {
        match &self.format {
            OutputFormat::Text { template: _ } => {
                if let Some(template) = &self.template {
                    let context = Context::from_serialize(json!({
                        "id": event.id,
                        "data": event.data,
                        "metadata": event.metadata,
                        "timestamp": chrono::Local::now().to_rfc3339()
                    }))?;
                    let output = template.render(&self.name, &context)?;
                    info!("{}", output);
                } else {
                    info!(
                        "Consuming event:\nID: {}\nData: {}\nMetadata: {}",
                        event.id, event.data, event.metadata
                    );
                }
            }
            OutputFormat::Json => {
                let json_output = to_string_pretty(&event)?;
                info!("Consuming event:\n{}", json_output);
            }
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
