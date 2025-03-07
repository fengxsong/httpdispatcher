mod console;
mod http;
mod render;

use crate::component::Sink;
use crate::config::SinkConfig;
use anyhow::{anyhow, Error, Result};

pub fn create_sink(name: String, config: SinkConfig) -> Result<Box<dyn Sink>, Error> {
    match config {
        SinkConfig::Http(config) => Ok(Box::new(http::HttpSink::new(name, config)?)),
        SinkConfig::Console(config) => {
            let encoding = config.encoding.clone().unwrap_or("text".to_string());
            let format = match encoding.as_str() {
                "json" => console::OutputFormat::Json,
                "text" => console::OutputFormat::Text {
                    template: config.template.clone(),
                },
                _ => return Err(anyhow!("Unknown encoding: {}", encoding)),
            };
            Ok(Box::new(console::ConsoleSink::new(name, format)?))
        }
    }
}
