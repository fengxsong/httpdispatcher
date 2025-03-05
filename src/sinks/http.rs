use crate::component::{Component, Event, Sink};
use crate::config::SinkConfig;
use crate::sinks::render::load_template;
use anyhow::{anyhow, Error, Result};
use async_trait::async_trait;
use handlebars::Handlebars;
use reqwest::{Client, Method, RequestBuilder};
use serde_json::json;
use std::any::Any;
use std::collections::HashMap;
use std::time::Duration;

pub struct HttpSink {
    name: String,
    type_: String,
    inputs: Vec<String>,
    client: Client,
    uri: String,
    method: String,
    headers: Option<HashMap<String, String>>,
    query_params: Option<HashMap<String, String>>,
    template: Option<String>,
    handlebars: Handlebars<'static>,
}

impl HttpSink {
    pub fn new(name: String, config: SinkConfig) -> Result<Self, Error> {
        let client = Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms.unwrap_or(30000)))
            .build()
            .unwrap();

        let mut hbs = Handlebars::new();
        if let Some(template) = &config.template {
            let template_content = load_template(template)?;
            hbs.register_template_string(&name, template_content)?;
        }

        Ok(Self {
            name,
            type_: "http".to_string(),
            inputs: config.inputs,
            client,
            uri: config.uri.unwrap(),
            method: config.method.unwrap_or_else(|| "POST".to_string()),
            headers: config.headers,
            query_params: config.query_params,
            template: config.template,
            handlebars: hbs,
        })
    }

    fn build_request(&self, event: &Event) -> Result<RequestBuilder, Error> {
        let template_data = json!({
            "id": event.id,
            "data": event.data,
            "metadata": event.metadata
        });

        let uri = self
            .handlebars
            .render_template(self.uri.as_str(), &template_data)?;

        let method = match self.method.to_uppercase().as_str() {
            "GET" => Method::GET,
            "POST" => Method::POST,
            "PUT" => Method::PUT,
            "DELETE" => Method::DELETE,
            "PATCH" => Method::PATCH,
            "HEAD" => Method::HEAD,
            "OPTIONS" => Method::OPTIONS,
            _ => Method::POST,
        };

        let mut request = self.client.request(method, &uri);

        if let Some(query_params) = &self.query_params {
            let mut rendered_params = HashMap::new();
            for (key, template) in query_params {
                let value = self
                    .handlebars
                    .render_template(template, &template_data)
                    .map_err(|e| anyhow!("Rendering query_params: {}", e))?;
                rendered_params.insert(key.clone(), value);
            }
            request = request.query(&rendered_params);
        }

        if let Some(headers) = &self.headers {
            for (key, template) in headers {
                let value = self
                    .handlebars
                    .render_template(template, &template_data)
                    .map_err(|e| anyhow!("Rendering headers: {}", e))?;
                request = request.header(key, value);
            }
        }

        if let Some(_) = &self.template {
            let body = self
                .handlebars
                .render(&self.name, &template_data)
                .map_err(|e| anyhow!("Rendering template: {}", e))?;
            request = request.body(body);
        } else {
            request = request.json(&event.data);
        }

        Ok(request)
    }
}

impl Component for HttpSink {
    fn name(&self) -> &str {
        &self.name
    }

    fn type_(&self) -> &str {
        &self.type_
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
