use anyhow::{Error, Result};
use tera::{Context, Tera};

pub fn load_template(template: &str) -> Result<String, Error> {
    if template.starts_with("file://") {
        let path = template.trim_start_matches("file://");
        Ok(std::fs::read_to_string(path)?)
    } else {
        Ok(template.to_string())
    }
}

#[allow(dead_code)]
pub fn render_str(template: &str, data: &serde_json::Value) -> Result<String, Error> {
    let template = load_template(template)?;
    let mut tera = Tera::default();
    let context = Context::from_value(data.clone())?;
    let rendered = tera.render_str(&template, &context)?;
    Ok(rendered)
}
