use crate::component::{Event, Sink, Source, Transform};
use crate::config::Config;
use anyhow::{anyhow, Error, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tracing::{error, info};

struct ComponentChannel {
    tx: broadcast::Sender<Event>,
    rx: broadcast::Receiver<Event>,
}

pub struct Runtime {
    config: Config,
    components: HashMap<String, ComponentRef>, // 统一组件存储
    channels: HashMap<String, ComponentChannel>, // 全局通道注册表
}

enum ComponentRef {
    Source(Arc<Mutex<Box<dyn Source>>>),
    Transform(Arc<Mutex<Box<dyn Transform>>>),
    Sink(Arc<Mutex<Box<dyn Sink>>>),
}

impl Runtime {
    pub async fn build(config: &Config) -> Result<Self, Error> {
        let mut runtime = Runtime {
            config: config.clone(),
            components: HashMap::new(),
            channels: HashMap::new(),
        };

        // sink 不需要 channel
        for name in config.sources.keys().chain(config.transforms.keys()) {
            let (sender, receiver) = broadcast::channel(100);
            runtime.channels.insert(
                name.to_string(),
                ComponentChannel {
                    tx: sender,
                    rx: receiver,
                },
            );
        }

        for (name, config) in &config.sources {
            let source = crate::sources::create_source(name.to_string(), config)?;
            runtime.components.insert(
                name.to_string(),
                ComponentRef::Source(Arc::new(Mutex::new(source))),
            );
        }

        for (name, config) in &config.transforms {
            let transform = crate::transforms::create_transform(name.to_string(), config)?;
            runtime.components.insert(
                name.to_string(),
                ComponentRef::Transform(Arc::new(Mutex::new(transform))),
            );
        }

        for (name, config) in &config.sinks {
            let sink = crate::sinks::create_sink(name.to_string(), config)?;
            runtime.components.insert(
                name.to_string(),
                ComponentRef::Sink(Arc::new(Mutex::new(sink))),
            );
        }

        Self::connect_pipelines(&mut runtime, config).await?;

        Ok(runtime)
    }

    async fn connect_pipelines(runtime: &mut Runtime, config: &Config) -> Result<(), Error> {
        // 连接 transforms 的输入
        for (name, t_config) in &config.transforms {
            {
                let transform = runtime.get_transform(name)?;

                {
                    let transform_channel = runtime.channels.get_mut(name).unwrap();
                    let tx = transform_channel.tx.clone();

                    for input in &t_config.inputs.clone() {
                        let input_channel = runtime.channels.get(input).unwrap();
                        let mut rx = input_channel.rx.resubscribe();
                        let tx_clone = tx.clone();
                        let transform_clone = transform.clone();
                        info!(
                            "Connecting transform {} to input {}",
                            transform_clone.lock().await.name(),
                            input
                        );
                        tokio::spawn(async move {
                            while let Ok(event) = rx.recv().await {
                                let processed =
                                    transform_clone.lock().await.transform(&event).await;
                                match processed {
                                    Ok(processed_event) => {
                                        let _ = tx_clone.send(processed_event);
                                    }
                                    Err(e) => error!(
                                        "Transform {} failed to process event: {:?}, err: {}",
                                        transform_clone.lock().await.name(),
                                        event,
                                        e
                                    ),
                                }
                            }
                        });
                    }
                }
            }
        }

        // 连接 sinks 的输入
        for (name, s_config) in &config.sinks {
            let sink = runtime.get_sink(name)?;

            for input in &s_config.inputs {
                let input_channel = runtime.channels.get(input).unwrap();
                let mut rx = input_channel.rx.resubscribe();
                let sink_clone = sink.clone();
                info!(
                    "Connecting sink {} to input {}",
                    sink_clone.lock().await.name(),
                    input
                );
                tokio::spawn(async move {
                    while let Ok(event) = rx.recv().await {
                        let processed = sink_clone.lock().await.process(&event).await;
                        match processed {
                            Ok(_) => {}
                            Err(e) => error!(
                                "Sink {} failed to process event: {:?}, err: {}",
                                sink_clone.lock().await.name(),
                                &event,
                                e
                            ),
                        }
                    }
                });
            }
        }

        Ok(())
    }

    pub async fn run(&self) -> Result<(), Error> {
        for (name, _) in &self.config.sources {
            let source = self.get_source(name)?;
            let channel = self.channels.get(name).unwrap().tx.clone();

            tokio::spawn(async move {
                let mut source = source.lock().await;
                let _ = source.run(channel).await;
            });
        }
        Ok(())
    }

    pub async fn shutdown(&mut self) -> Result<(), Error> {
        for name in self.config.sources.keys() {
            let source = self.get_source(name)?;
            let mut source = source.lock().await;
            if let Err(e) = source.shutdown().await {
                error!("Error stopping source: {}", e);
            }
        }
        Ok(())
    }
}

impl Runtime {
    fn get_source(&self, name: &str) -> Result<Arc<Mutex<Box<dyn Source>>>, Error> {
        match self.components.get(name) {
            Some(ComponentRef::Source(s)) => Ok(s.clone()),
            _ => Err(anyhow!("Source {} not found", name)),
        }
    }

    fn get_transform(&self, name: &str) -> Result<Arc<Mutex<Box<dyn Transform>>>, Error> {
        match self.components.get(name) {
            Some(ComponentRef::Transform(t)) => Ok(t.clone()),
            _ => Err(anyhow!("Transform {} not found", name)),
        }
    }

    fn get_sink(&self, name: &str) -> Result<Arc<Mutex<Box<dyn Sink>>>, Error> {
        match self.components.get(name) {
            Some(ComponentRef::Sink(s)) => Ok(s.clone()),
            _ => Err(anyhow!("Sink {} not found", name)),
        }
    }
}
