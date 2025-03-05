// 添加文件开头的 #[allow(dead_code)] 注解
#[allow(dead_code)]
use crate::component::Event;
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use anyhow::{Result, Error};

pub struct TemplateEngine {
    regex: Regex,
}

impl TemplateEngine {
    pub fn new() -> Self {
        let regex = Regex::new(r"\{\{\s*([^}]+)\s*\}\}").unwrap();
        Self { regex }
    }

    pub fn render(&self, template: &str, event: &Event) -> Result<String, Box<dyn Error>> {
        let mut result = template.to_string();
        
        for cap in self.regex.captures_iter(template) {
            let full_match = cap[0].to_string();
            let path = cap[1].trim();
            
            let value = self.extract_value_from_path(path, event)?;
            result = result.replace(&full_match, &value);
        }
        
        Ok(result)
    }
    
    pub fn render_map(&self, templates: &HashMap<String, String>, event: &Event) 
        -> Result<HashMap<String, String>, Box<dyn Error>> {
        let mut result = HashMap::new();
        
        for (key, template) in templates {
            let rendered = self.render(template, event)?;
            result.insert(key.clone(), rendered);
        }
        
        Ok(result)
    }

    fn extract_value_from_path(&self, path: &str, event: &Event) -> Result<String, Box<dyn Error>> {
        // 支持特殊路径如 "data", "metadata", "id"
        if path == "id" {
            return Ok(event.id.clone());
        }
        
        // 处理嵌套路径，如 "data.user.name"
        let parts: Vec<&str> = path.split('.').collect();
        let root = match parts[0] {
            "data" => &event.data,
            "metadata" => &event.metadata,
            _ => return Err(format!("未知的根路径: {}", parts[0]).into()),
        };
        
        let mut current = root;
        for &part in parts.iter().skip(1) {
            match current.get(part) {
                Some(value) => current = value,
                None => return Err(format!("路径 '{}' 在 '{}' 中不存在", part, path).into()),
            }
        }
        
        match current {
            Value::String(s) => Ok(s.clone()),
            Value::Number(n) => Ok(n.to_string()),
            Value::Bool(b) => Ok(b.to_string()),
            Value::Null => Ok("null".to_string()),
            _ => Ok(current.to_string()),
        }
    }
}