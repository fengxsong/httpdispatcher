use anyhow::{Error, Result};
use clap::Parser;
use httpdispatcher::Runtime;
use tracing::info;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

fn print_version() {
    println!("HTTP Dispatcher {}", env!("CARGO_PKG_VERSION"));
    println!("Build Time:    {}", env!("BUILD_TIME"));
    println!("Git Commit:    {}", env!("GIT_COMMIT_HASH"));
    println!("Rust Version:  {}", env!("RUSTC_VERSION"));
    println!("Build Profile: {}", env!("BUILD_PROFILE"));
}

#[derive(Parser)]
#[command(author, version, about, long_about = None, disable_version_flag=true)]
struct Args {
    #[arg(short, long, default_value = "config.toml")]
    config: String,

    #[arg(short, long, default_value = "info")]
    log_level: String,

    #[arg(long, default_value = "text", value_parser = ["text", "json"])]
    log_format: String,

    #[arg(long, short = 'v', help = "Print version information")]
    version: bool,

    #[arg(
        long,
        default_value = "true",
        hide = true,
        help = "Enable configuration auto-reload"
    )]
    auto_reload: bool,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args = Args::parse();

    if args.version {
        print_version();
        return Ok(());
    }

    // Initialize logging
    let fmt_layer = match args.log_format.as_str() {
        "json" => Box::new(fmt::layer().json()),
        _ => fmt::layer().boxed(),
    };

    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(&args.log_level))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .init();

    // Set auto-reload environment variable if enabled
    if args.auto_reload {
        std::env::set_var("DISPATCHER_ENABLE_AUTO_RELOAD", "1");
    }

    // Build and start runtime
    info!("Building runtime...");
    let mut runtime = Runtime::build(args.config).await?;

    info!("Starting pipeline execution...");
    runtime.run().await?;

    // Keep the application running
    tokio::signal::ctrl_c().await?;

    info!("Shutting down...");
    runtime.shutdown().await?;

    Ok(())
}
