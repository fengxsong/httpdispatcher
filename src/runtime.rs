use crate::component::{Event, Sink, Source, Transform};
use crate::config::Config;
use anyhow::{anyhow, Context, Error, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tracing::{error, info};


pub struct Runtime {
    config: Config,
    tx: HashMap<String, broadcast::Sender<Event>>,
    rx: HashMap<String, broadcast::Receiver<Event>>,
    components: HashMap<String, ComponentRef>,
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
            tx: Default::default(),
            rx: Default::default(),
        };

        config.sources.keys().chain(config.transforms.keys()).into_iter().for_each(|name| {
            let (sender, receiver) = broadcast::channel(100);
            runtime.tx.insert(name.to_string(), sender);
            runtime.rx.insert(name.to_string(), receiver);
        });

        for (name, config) in &config.sources {
            let source = crate::sources::create_source(name.to_string(), config.clone())?;
            runtime.components.insert(
                name.to_string(),
                ComponentRef::Source(Arc::new(Mutex::new(source))),
            );
        }

        for (name, config) in &config.transforms {
            let transform = crate::transforms::create_transform(name.to_string(), config.clone())?;
            runtime.components.insert(
                name.to_string(),
                ComponentRef::Transform(Arc::new(Mutex::new(transform))),
            );
        }

        for (name, config) in &config.sinks {
            let sink = crate::sinks::create_sink(name.to_string(), config.clone())?;
            runtime.components.insert(
                name.to_string(),
                ComponentRef::Sink(Arc::new(Mutex::new(sink))),
            );
        }

        Self::connect_pipelines(&mut runtime, config).await?;

        Ok(runtime)
    }

    async fn connect_pipelines(runtime: &mut Runtime, config: &Config) -> Result<(), Error> {
        for (name, c) in &runtime.components {
            match c {
                ComponentRef::Transform(t) => {
                    let transform = t.clone();
                    let tx = runtime.tx.get(name).unwrap().clone();
                    let inputs = {
                        let guard = transform.lock().await;
                        guard.inputs().to_vec()
                    };
                    for input in inputs {
                        if !runtime.components.contains_key(&input) {
                            return Err(anyhow!("Transform {} has input {} which is not a component",name,input));
                        }
                        let input_rx = runtime.rx.get(&input).unwrap().clone();
                        {
                            let guard = transform.lock().await;
                            info!(
                                "Connecting transform {} to input {}",
                                guard.name(),
                                input
                            );
                        }
                        let transform_clone = transform.clone();
                        tokio::spawn(async move {
                            let mut rx = input_rx;
                            while let Ok(event) = rx.recv().await {
                                let processed = {
                                    let mut guard = transform_clone.lock().await;
                                    guard.transform(&event).await
                                };
                                
                                match processed {
                                    Ok(processed_event) => {
                                        let _ = tx.send(processed_event);
                                    }
                                    Err(e) => {
                                        let name = transform_clone.lock().await.name();
                                        error!(
                                            "Transform {} failed to process event: {:?}, err: {}",
                                            name,
                                            event.id,
                                            e
                                        );
                                    }
                                }
                            }
                        });
                    }
                }
                ComponentRef::Sink(s) => {
                    let sink = s.clone(); // 克隆 Arc<Mutex<Box<dyn Sink>>>
                    
                    // 获取输入列表
                    let inputs = {
                        let guard = sink.lock().await;
                        guard.inputs().to_vec() // 复制输入列表，避免持有锁
                    };
                    
                    for input in inputs {
                        if !runtime.components.contains_key(&input) {
                            return Err(anyhow!(
                                "Sink {} has input {} which is not a component",
                                name,
                                input
                            ));
                        }
                        let input_rx = runtime.rx.get(&input).unwrap().resubscribe();
                        
                        // 记录连接信息
                        {
                            let guard = sink.lock().await;
                            info!(
                                "Connecting sink {} to input {}",
                                guard.name(),
                                input
                            );
                        }
                        
                        let sink_clone = sink.clone();
                        tokio::spawn(async move {
                            let mut rx = input_rx;
                            while let Ok(event) = rx.recv().await {
                                let processed = {
                                    let mut guard = sink_clone.lock().await;
                                    guard.process(&event).await
                                };
                                
                                match processed {
                                    Ok(_) => {}
                                    Err(e) => {
                                        let name = sink_clone.lock().await.name();
                                        error!(
                                            "Sink {} failed to process event: {:?}, err: {}",
                                            name,
                                            &event.id,
                                            e
                                        );
                                    }
                                }
                            }
                        });
                    }
                }
                _ => {}
            }
        }
 
        Ok(())
    }

    pub async fn run(&self) -> Result<(), Error> {
        for (name, _) in &self.config.sources {
            let source = self.get_source(name)?;
            let channel = self.tx.get(name).unwrap().clone();

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
