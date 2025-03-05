use crate::component::{Component, Event, Sink};
use crate::config::SinkConfig;
use crate::sinks::render::load_template;
use anyhow::{anyhow, Error, Result};
use async_trait::async_trait;
use reqwest::{Client, Method, RequestBuilder};
use serde_json::json;
use std::any::Any;
use std::collections::HashMap;
use std::time::Duration;
use tera::{Context, Tera};

pub struct HttpSink {
    name: String,
    inputs: Vec<String>,
    client: Client,
    uri: String,
    method: Option<String>,
    headers: Option<HashMap<String, String>>,
    query_params: Option<HashMap<String, String>>,
    template: Tera,
}

impl HttpSink {
    pub fn new(name: String, config: SinkConfig) -> Result<Self, Error> {
        let client = Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms.unwrap_or(30000)))
            .build()
            .unwrap();

        let mut template = Tera::default();
        if let Some(tpl) = &config.template {
            let template_content = load_template(tpl)?;
            template.add_raw_template(&name, template_content.as_str())?;
        }

        Ok(Self {
            name,
            inputs: config.inputs,
            client,
            uri: config.uri.unwrap(),
            method: config.method,
            headers: config.headers,
            query_params: config.query_params,
            template: template,
        })
    }

    fn build_request(&self, event: &Event) -> Result<RequestBuilder, Error> {
        let template_data = Context::from_serialize(json!({
            "id": event.id,
            "data": event.data,
            "metadata": event.metadata
        }))?;

        let uri = self
            .template
            .clone()
            .render_str(self.uri.as_str(), &template_data)?;

        let method = {
            if let Some(method) = self.method.clone() {
                match method.to_uppercase().as_str() {
                    "GET" => Method::GET,
                    "POST" => Method::POST,
                    "PUT" => Method::PUT,
                    "DELETE" => Method::DELETE,
                    "PATCH" => Method::PATCH,
                    "HEAD" => Method::HEAD,
                    "OPTIONS" => Method::OPTIONS,
                    _ => Method::POST,
                }
            } else {
                Method::POST
            }
        };

        let mut request = self.client.request(method, &uri);

        if let Some(query_params) = &self.query_params {
            let mut rendered_params = HashMap::new();
            for (key, template) in query_params {
                let value = self
                    .template
                    .clone()
                    .render_str(template, &template_data)
                    .map_err(|e| anyhow!("Rendering query_params: {}", e))?;
                rendered_params.insert(key.clone(), value);
            }
            request = request.query(&rendered_params);
        }

        if let Some(headers) = &self.headers {
            for (key, template) in headers {
                let value = self
                    .template
                    .clone()
                    .render_str(template, &template_data)
                    .map_err(|e| anyhow!("Rendering headers: {}", e))?;
                request = request.header(key, value);
            }
        }
        let body = self.template.render(&self.name, &template_data)?;
        request = request.body(body);

        Ok(request)
    }
}

impl Component for HttpSink {
    fn name(&self) -> &str {
        &self.name
    }

    fn type_(&self) -> &str {
        "http"
    }

    fn inputs(&self) -> &[String] {
        &self.inputs
    }
}

#[async_trait]
impl Sink for HttpSink {
    async fn process(&mut self, event: &Event) -> Result<(), Error> {
        let request = self.build_request(&event)?;
        let response = request.send().await?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "HTTP sink request failed with status: {}",
                response.status()
            ));
        }

        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
