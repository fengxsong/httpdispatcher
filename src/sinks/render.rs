pub fn load_template(template: &str) -> Result<String, anyhow::Error> {
    if template.starts_with("file://") {
        let path = template.trim_start_matches("file://");
        Ok(std::fs::read_to_string(path)?)
    } else {
        Ok(template.to_string())
    }
}
