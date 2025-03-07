use crate::component::{Component, Event, Source};
use crate::config::HttpSourceConfig;
use anyhow::{Error, Result};
use async_trait::async_trait;
use axum::{
    extract::{DefaultBodyLimit, Request},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, Router},
    Json,
};
use nanoid::nanoid;
use serde_json::{json, Value};
use std::any::Any;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::usize;
use tokio::sync::broadcast;
use tower::ServiceBuilder;
use tower_http::classify::ServerErrorsFailureClass;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info, warn};

pub struct HttpSource {
    name: String,
    config: HttpSourceConfig,
    tx: Option<broadcast::Sender<Event>>,
    server_handle: Option<tokio::task::JoinHandle<()>>,
}

impl Clone for HttpSource {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            config: self.config.clone(),
            tx: self.tx.clone(),
            server_handle: None,
        }
    }
}

impl HttpSource {
    pub fn new(name: String, config: HttpSourceConfig) -> Result<Self, Error> {
        Ok(Self {
            name,
            config,
            tx: None,
            server_handle: None,
        })
    }

    async fn handle_echo(req: Request<axum::body::Body>) -> impl IntoResponse {
        let (parts, body) = req.into_parts();
        let headers = parts.headers;
        let uri = parts.uri;
        let method = parts.method;

        let body = match axum::body::to_bytes(body, usize::MAX).await {
            Ok(b) => b,
            Err(e) => {
                error!("Failed to read body: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": e.to_string()})),
                )
                    .into_response();
            }
        };

        let headers: HashMap<String, String> = headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or_default().to_string()))
            .collect();

        let body_json = serde_json::from_slice::<Value>(&body).unwrap_or(Value::Null);

        let request_info = json!({
            "method": method.to_string(),
            "uri": uri.to_string(),
            "path": uri.path(),
            "headers": headers,
            "query": uri.query().unwrap_or_default(),
            "version": format!("{:?}", parts.version),
            "body": body_json
        });

        (StatusCode::OK, Json(request_info)).into_response()
    }

    async fn handle_request(
        Json(payload): Json<Value>,
        tx: Arc<broadcast::Sender<Event>>,
    ) -> impl IntoResponse {
        let event = Event {
            id: nanoid!(16),
            data: payload,
            metadata: Value::Object(Default::default()),
        };

        match tx.send(event.clone()) {
            Ok(_) => {
                debug!("Successfully processed event: {}", &event.id);
                (
                    StatusCode::OK,
                    Json(serde_json::json!({ "event_id": &event.id })),
                )
                    .into_response()
            }
            Err(e) => {
                error!("Failed to process event {}: {}", &event.id, e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!(
                        { "event_id": &event.id, "error": e.to_string() }
                    )),
                )
                    .into_response()
            }
        }
    }
}

impl Component for HttpSource {
    fn name(&self) -> &str {
        &self.name
    }

    fn type_(&self) -> &str {
        "http"
    }

    fn inputs(&self) -> &[String] {
        &[]
    }
}

#[async_trait]
impl Source for HttpSource {
    async fn run(&mut self, tx: broadcast::Sender<Event>) -> Result<(), Error> {
        self.tx = Some(tx.clone());

        let addr: SocketAddr = format!(
            "{}:{}",
            self.config.address,
            self.config.port.unwrap_or(3000)
        )
        .parse()?;

        let tx = Arc::new(tx);
        let tx_clone = tx.clone();

        let mut app = Router::new()
            .route(
                &self.config.path,
                post(move |payload| HttpSource::handle_request(payload, tx_clone)),
            )
            .layer(
                TraceLayer::new_for_http()
                    .on_request(|request: &axum::http::Request<_>, _span: &tracing::Span| {
                        debug!("Received request: {} {}", request.method(), request.uri());
                    })
                    .on_response(
                        |response: &axum::http::Response<_>,
                         latency: std::time::Duration,
                         _span: &tracing::Span| {
                            debug!("Response generated in {:?}: {}", latency, response.status());
                        },
                    )
                    .on_failure(
                        |error: ServerErrorsFailureClass,
                         _latency: std::time::Duration,
                         _span: &tracing::Span| {
                            warn!("Request failed: {:?}", error);
                        },
                    ),
            )
            .layer(
                ServiceBuilder::new()
                    .layer(DefaultBodyLimit::max(
                        self.config.max_body_size_bytes.unwrap_or(1024 * 1024),
                    ))
                    .layer(RequestBodyLimitLayer::new(
                        self.config.max_body_size_bytes.unwrap_or(1024 * 1024),
                    )),
            );

        if self.config.enable_echo.unwrap_or(false) {
            app = app.route("/echo", get(Self::handle_echo).post(Self::handle_echo))
        }

        let listener = tokio::net::TcpListener::bind(&addr).await?;

        let server = axum::serve(listener, app);

        info!("HTTP source listening on {}", addr);

        self.server_handle = Some(tokio::spawn(async move {
            if let Err(e) = server.await {
                error!("HTTP server error: {}", e);
            }
        }));

        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), Error> {
        if let Some(handle) = self.server_handle.take() {
            handle.abort();
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
