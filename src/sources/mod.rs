mod http;

use crate::component::Source;
use crate::config::SourceConfig;
use anyhow::{anyhow, Error, Result};

pub use http::HttpSource;

pub fn create_source(name: String, config: &SourceConfig) -> Result<Box<dyn Source>, Error> {
    match config.type_.as_str() {
        "http" => Ok(Box::new(HttpSource::new(name, config.clone())?)),
        _ => Err(anyhow!("Unknown source type: {}", config.type_)),
    }
}
