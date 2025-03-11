mod console;
mod http;
mod render;

use crate::component::Sink;
use crate::config::SinkConfig;
use anyhow::{Error, Result};

pub fn create_sink(name: String, config: SinkConfig) -> Result<Box<dyn Sink>, Error> {
    match config {
        SinkConfig::Http(config) => Ok(Box::new(http::HttpSink::new(name, config)?)),
        SinkConfig::Console(config) => Ok(Box::new(console::ConsoleSink::new(name, config)?)),
    }
}
