use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Cli {
    /// Path to the configuration file
    #[arg(short, long, default_value = "config.toml")]
    pub config: PathBuf,

    /// Server host address
    #[arg(long)]
    pub host: Option<String>,

    /// Server port
    #[arg(long)]
    pub port: Option<u16>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, default_value = "info")]
    pub log_level: String,

    /// Print the configuration and exit
    #[arg(short, long)]
    pub print_config: bool,
}

impl Cli {
    pub fn load() -> Self {
        Self::parse()
    }

    pub fn get_log_filter(&self) -> String {
        format!("httpdispacher={}", self.log_level)
    }
} 