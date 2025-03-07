use crate::component::{Event, Sink, Source, Transform};
use crate::config::{Config, ConfigManager};
use crate::error::{Result, RuntimeError};
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tracing::{error, info};

pub struct Runtime {
    config_manager: Arc<ConfigManager>,
    tx: DashMap<String, broadcast::Sender<Event>>,
    rx: DashMap<String, broadcast::Receiver<Event>>,
    components: DashMap<String, ComponentRef>,
}

#[derive(Clone)]
enum ComponentRef {
    Source(Arc<Mutex<Box<dyn Source>>>),
    Transform(Arc<Mutex<Box<dyn Transform>>>),
    Sink(Arc<Mutex<Box<dyn Sink>>>),
}

impl Runtime {
    pub async fn build(config: &Config, config_path: String) -> Result<Self> {
        let config_manager = Arc::new(ConfigManager::new(config_path, config.clone()));
        
        let runtime = Runtime {
            config_manager: config_manager.clone(),
            components: DashMap::new(),
            tx: DashMap::new(),
            rx: DashMap::new(),
        };

        // Initialize components
        runtime.initialize_components().await?;

        // Start configuration auto-reload if enabled
        if std::env::var("DISPATCHER_ENABLE_AUTO_RELOAD").is_ok() {
            info!("Starting configuration auto-reload");
            config_manager.clone().start_auto_reload().await;
        }

        Ok(runtime)
    }

    async fn initialize_components(&self) -> Result<()> {
        let config = self.config_manager.get_config().await;
        let config = config.read().await;

        // Initialize channels
        config
            .sources
            .keys()
            .chain(config.transforms.keys())
            .for_each(|name| {
                let (sender, receiver) = broadcast::channel(config.channel_capacity);
                self.tx.insert(name.to_string(), sender);
                self.rx.insert(name.to_string(), receiver);
            });

        // Create sources
        for (name, source_config) in &config.sources {
            let source = crate::sources::create_source(name.to_string(), source_config.clone())
                .map_err(|e| RuntimeError::init_error(format!("Failed to create source {}: {}", name, e)))?;
            self.components.insert(
                name.to_string(),
                ComponentRef::Source(Arc::new(Mutex::new(source))),
            );
        }

        // Create transforms
        for (name, transform_config) in &config.transforms {
            let transform = crate::transforms::create_transform(name.to_string(), transform_config.clone())
                .map_err(|e| RuntimeError::init_error(format!("Failed to create transform {}: {}", name, e)))?;
            self.components.insert(
                name.to_string(),
                ComponentRef::Transform(Arc::new(Mutex::new(transform))),
            );
        }

        // Create sinks
        for (name, sink_config) in &config.sinks {
            let sink = crate::sinks::create_sink(name.to_string(), sink_config.clone())
                .map_err(|e| RuntimeError::init_error(format!("Failed to create sink {}: {}", name, e)))?;
            self.components.insert(
                name.to_string(),
                ComponentRef::Sink(Arc::new(Mutex::new(sink))),
            );
        }

        Runtime::connect_pipelines(self).await?;

        Ok(())
    }

    pub async fn reload(&self) -> Result<()> {
        info!("Reloading runtime configuration");
        
        // Reload configuration
        self.config_manager.reload().await?;
        
        // Clear existing components
        self.components.clear();
        self.tx.clear();
        self.rx.clear();
        
        // Initialize with new configuration
        self.initialize_components().await?;
        
        info!("Runtime configuration reloaded successfully");
        Ok(())
    }

    async fn connect_pipelines(runtime: &Runtime) -> Result<()> {
        // Store all spawned tasks
        let mut tasks = Vec::new();

        for pair in runtime.components.iter() {
            let name = pair.key().clone();
            let component = pair.value();
            
            match component {
                ComponentRef::Transform(transform) => {
                    let transform = transform.clone();

                    // Get inputs before spawning tasks
                    let inputs = {
                        let guard = transform.lock().await;
                        guard.inputs().to_vec()
                    };

                    for input in inputs {
                        if !runtime.components.contains_key(&input) {
                            return Err(RuntimeError::invalid_input(
                                name.clone(),
                                format!("Input {} not found", input),
                            ));
                        }

                        let tx = runtime.tx.get(&name).ok_or_else(|| {
                            RuntimeError::channel_error(format!("Channel not found for {}", name))
                        })?.value().clone();
                        
                        let mut input_rx = runtime.rx.get(&input).ok_or_else(|| {
                            RuntimeError::channel_error(format!("Channel not found for input {}", input))
                        })?.value().resubscribe();
                        
                        let transform = transform.clone();

                        // Log connection
                        {
                            let guard = transform.lock().await;
                            info!("Connecting transform {} to input {}", guard.name(), input);
                        }

                        let task = tokio::spawn(async move {
                            while let Ok(event) = input_rx.recv().await {
                                let processed = {
                                    let guard = transform.lock().await;
                                    guard.transform(&event).await
                                };

                                match processed {
                                    Ok(processed_event) => {
                                        let _ = tx.send(processed_event);
                                    }
                                    Err(e) => {
                                        let guard = transform.lock().await;
                                        error!(
                                            "Transform {} failed to process event: {:?}, err: {}",
                                            guard.name(),
                                            event.id,
                                            e
                                        );
                                    }
                                }
                            }
                        });

                        tasks.push(task);
                    }
                }
                ComponentRef::Sink(sink) => {
                    let sink = sink.clone();

                    // Get inputs before spawning tasks
                    let inputs = {
                        let guard = sink.lock().await;
                        guard.inputs().to_vec()
                    };

                    for input in inputs {
                        if !runtime.components.contains_key(&input) {
                            return Err(RuntimeError::invalid_input(
                                name.clone(),
                                format!("Input {} not found", input),
                            ));
                        }

                        let mut input_rx = runtime.rx.get(&input).ok_or_else(|| {
                            RuntimeError::channel_error(format!("Channel not found for input {}", input))
                        })?.value().resubscribe();
                        
                        let sink = sink.clone();

                        // Log connection
                        {
                            let guard = sink.lock().await;
                            info!("Connecting sink {} to input {}", guard.name(), input);
                        }

                        let task = tokio::spawn(async move {
                            while let Ok(event) = input_rx.recv().await {
                                let processed = {
                                    let mut guard = sink.lock().await;
                                    guard.process(&event).await
                                };

                                if let Err(e) = processed {
                                    let guard = sink.lock().await;
                                    error!(
                                        "Sink {} failed to process event: {:?}, err: {}",
                                        guard.name(),
                                        event.id,
                                        e
                                    );
                                }
                            }
                        });

                        tasks.push(task);
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    pub async fn run(&self) -> Result<()> {
        for (name, _) in &self.config_manager.get_config().await.read().await.sources {
            let source = self.get_source(name)?;
            let channel = self.tx.get(name).ok_or_else(|| {
                RuntimeError::channel_error(format!("Channel not found for source {}", name))
            })?.value().clone();

            tokio::spawn(async move {
                let mut source = source.lock().await;
                let _ = source.run(channel).await;
            });
        }
        Ok(())
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        for name in self.config_manager.get_config().await.read().await.sources.keys() {
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
    fn get_source(&self, name: &str) -> Result<Arc<Mutex<Box<dyn Source>>>> {
        match self.components.get(name).as_deref() {
            Some(ComponentRef::Source(s)) => Ok(s.clone()),
            _ => Err(RuntimeError::component_not_found(name.to_string())),
        }
    }

    #[allow(dead_code)]
    fn get_transform(&self, name: &str) -> Result<Arc<Mutex<Box<dyn Transform>>>> {
        match self.components.get(name).as_deref() {
            Some(ComponentRef::Transform(t)) => Ok(t.clone()),
            _ => Err(RuntimeError::component_not_found(name.to_string())),
        }
    }

    #[allow(dead_code)]
    fn get_sink(&self, name: &str) -> Result<Arc<Mutex<Box<dyn Sink>>>> {
        match self.components.get(name).as_deref() {
            Some(ComponentRef::Sink(s)) => Ok(s.clone()),
            _ => Err(RuntimeError::component_not_found(name.to_string())),
        }
    }
}
