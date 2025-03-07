mod vrl;

use crate::component::Transform;
use crate::config::TransformConfig;
use anyhow::{Error, Result};

pub use vrl::VrlTransform;

pub fn create_transform(
    name: String,
    config: TransformConfig,
) -> Result<Box<dyn Transform>, Error> {
    match config {
        TransformConfig::Remap(config) => Ok(Box::new(VrlTransform::new(name, config)?)),
    }
}
