mod http;

use crate::component::Source;
use crate::config::SourceConfig;
use anyhow::{Error, Result};

pub use http::HttpSource;

pub fn create_source(name: String, config: SourceConfig) -> Result<Box<dyn Source>, Error> {
    match config {
        SourceConfig::Http(config) => Ok(Box::new(HttpSource::new(name, config)?)),
    }
}
