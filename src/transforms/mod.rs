mod route;
mod vrl;

use crate::component::{Event, Transform};
use crate::config::TransformConfig;
use anyhow::{Error, Result};

use dashmap::DashMap;
use tokio::sync::broadcast;
pub use vrl::VrlTransform;

pub fn create_transform(
    name: String,
    config: TransformConfig,
    output_channel: DashMap<String, broadcast::Sender<Event>>,
) -> Result<Box<dyn Transform>, Error> {
    match config {
        TransformConfig::Remap(config) => Ok(Box::new(VrlTransform::new(name, config)?)),
        TransformConfig::Route(config) => Ok(Box::new(route::RouteTransform::new(
            name,
            config,
            output_channel,
        )?)),
    }
}
