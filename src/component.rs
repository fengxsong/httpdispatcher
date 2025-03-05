#![allow(dead_code)]
use anyhow::{Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::any::Any;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub data: Value,
    pub metadata: Value,
}

pub trait Component {
    fn name(&self) -> &str;
    fn type_(&self) -> &str;
    fn inputs(&self) -> &[String];
}

#[async_trait]
pub trait Source: Component + Send + 'static {
    async fn run(&mut self, tx: broadcast::Sender<Event>) -> Result<(), Error>;
    async fn shutdown(&mut self) -> Result<(), Error>;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

#[async_trait]
pub trait Transform: Component + Send + 'static {
    async fn transform(&self, event: &Event) -> Result<Event, Error>;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

#[async_trait]
pub trait Sink: Component + Send + 'static {
    async fn process(&mut self, event: &Event) -> Result<(), Error>;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
