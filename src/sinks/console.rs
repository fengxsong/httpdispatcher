use crate::component::{Component, Event, Sink};
use crate::config::ConsoleSinkConfig;
use anyhow::{anyhow, Context, Error, Result};
use async_trait::async_trait;
use serde_json::to_string_pretty;
use std::any::Any;
use tera::Tera;
use tracing::info;

use super::render::load_template;

#[derive(Debug)]
pub struct ConsoleSink {
    name: String,
    inputs: Vec<String>,
    format: OutputFormat,
    template: Option<Tera>,
}

#[derive(Debug, Clone)]
pub enum OutputFormat {
    Text { template: Option<String> },
    Json,
}

impl ConsoleSink {
    pub fn new(name: String, config: ConsoleSinkConfig) -> Result<Self, Error> {
        let encoding = config.encoding.clone().unwrap_or("text".to_string());
        let format = match encoding.as_str() {
            "json" => OutputFormat::Json,
            "text" => OutputFormat::Text {
                template: config.template.clone(),
            },
            _ => return Err(anyhow!("Unknown encoding: {}", encoding)),
        };

        let mut template = None;
        if let OutputFormat::Text {
            template: Some(tpl),
        } = &format
        {
            let content = load_template(tpl)?;
            let mut tera = Tera::default();
            tera.add_raw_template(&name, &content)
                .context(anyhow!("Failed to add template"))?;
            template = Some(tera);
        };

        Ok(Self {
            name,
            inputs: config.inputs.clone(),
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
        &self.inputs
    }
}

#[async_trait]
impl Sink for ConsoleSink {
    async fn process(&mut self, event: &Event) -> Result<(), Error> {
        match &self.format {
            OutputFormat::Text { template: _ } => {
                if let Some(template) = &self.template {
                    let context = tera::Context::from_serialize(&event.data)?;
                    let output = template
                        .render(&self.name, &context)
                        .map_err(|e| anyhow!("{:?}", e))?;
                    info!("{}", output);
                } else {
                    info!(
                        "Consuming event:\nID: {}\nData: {}\nMetadata: {}",
                        event.id, event.data, event.metadata
                    );
                }
            }
            OutputFormat::Json => {
                let json_output = to_string_pretty(&event.data)?;
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
