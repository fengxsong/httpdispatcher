use crate::component::{Component, Event, Sink};
use crate::config::HttpSinkConfig;
use crate::sinks::render::load_template;
use anyhow::{anyhow, Context, Error, Result};
use async_trait::async_trait;
use reqwest::{Certificate, Client, Method, RequestBuilder};
use std::collections::HashMap;
use std::time::Duration;
use std::{any::Any, fs};
use tera::Tera;
use tracing::error;

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
    pub fn new(name: String, config: HttpSinkConfig) -> Result<Self, Error> {
        let mut client_builder = Client::builder()
            .redirect(match config.follow_redirects {
                true => reqwest::redirect::Policy::default(),
                false => reqwest::redirect::Policy::none(),
            })
            .timeout(Duration::from_millis(config.timeout_ms.unwrap_or(30000)));

        if let Some(proxy) = &config.proxy_url {
            client_builder = client_builder.proxy(reqwest::Proxy::all(proxy)?);
        } else if config.proxy_from_environment {
            // TODO: implement
        }

        if let Some(tls) = config.tls {
            if let Some(ca_file) = &tls.ca_file {
                let certs = fs::read(ca_file)?;
                client_builder =
                    client_builder.add_root_certificate(Certificate::from_pem(&certs)?);
            }
            if tls.client_cert.is_some() && tls.client_key.is_some() {
                let client_cert = fs::read(tls.client_cert.as_deref().unwrap_or_default())?;
                let client_key = fs::read(tls.client_key.as_deref().unwrap_or_default())?;
                let tls_config = reqwest::Identity::from_pkcs8_pem(&client_cert, &client_key)?;

                client_builder = client_builder
                    .tls_built_in_root_certs(!tls.verify)
                    .identity(tls_config);
            }
        }

        let mut template = Tera::default();
        if let Some(tpl) = &config.template {
            let template_content = load_template(tpl)?;
            template.add_raw_template(&name, template_content.as_str())?;
        }

        let client = client_builder
            .build()
            .context(anyhow!("Creating HTTP client"))?;

        Ok(Self {
            name,
            inputs: config.inputs,
            client,
            uri: config.uri,
            method: config.method,
            headers: config.headers,
            query_params: config.query_params,
            template,
        })
    }

    fn build_request(&self, event: &Event) -> Result<RequestBuilder, Error> {
        let ctx = tera::Context::from_serialize(&event.data)?;

        let uri = self
            .template
            .clone()
            .render_str(self.uri.as_str(), &ctx)
            .map_err(|e| anyhow!("Rendering uri: {:?}", e))?;

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
                    .render_str(template, &ctx)
                    .map_err(|e| anyhow!("Rendering query_params: {:?}", e))?;
                rendered_params.insert(key.clone(), value);
            }
            request = request.query(&rendered_params);
        }

        if let Some(headers) = &self.headers {
            for (key, template) in headers {
                let value = self
                    .template
                    .clone()
                    .render_str(template, &ctx)
                    .map_err(|e| anyhow!("Rendering headers: {:?}", e))?;
                request = request.header(key, value);
            }
        }
        let body = self
            .template
            .render(&self.name, &ctx)
            .map_err(|e| anyhow!("Rendering body: {:?}", e))?;
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
            let status = response.status();
            let body = response.text().await?;
            error!(
                "HTTP sink request failed with status: {}, response: {}",
                status, body
            );
            return Err(anyhow!("HTTP sink request failed with status: {}", status));
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
