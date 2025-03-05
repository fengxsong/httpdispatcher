mod vrl;

use crate::component::Transform;
use crate::config::TransformConfig;
use anyhow::{anyhow, Error, Result};

pub use vrl::VrlTransform;

pub fn create_transform(
    name: String,
    config: &TransformConfig,
) -> Result<Box<dyn Transform>, Error> {
    match config.type_.as_str() {
        "remap" => Ok(Box::new(VrlTransform::new(name, config)?)),
        _ => Err(anyhow!("Unknown transform type: {}", config.type_)),
    }
}
