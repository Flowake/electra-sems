use clap::Parser;
use sems_api::create_app;
use sems_core::{StationConfig, StationState};
use std::path::PathBuf;

/// Command line arguments for the electra-sems server
#[derive(Parser, Debug)]
#[command(name = "electra-sems")]
#[command(about = "Electra Station Energy Management System")]
struct Args {
    /// Path to the station configuration JSON file
    #[arg(short, long)]
    config: PathBuf,

    /// Port to bind the server to
    #[arg(short, long, default_value = "3000")]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let args = Args::parse();

    // Initialize tracing
    tracing_subscriber::fmt().pretty().init();

    // Load station configuration from JSON file
    let config_content = tokio::fs::read_to_string(&args.config).await.map_err(|e| {
        format!(
            "Failed to read config file '{}': {}",
            args.config.display(),
            e
        )
    })?;

    let station_config: StationConfig = serde_json::from_str(&config_content).map_err(|e| {
        format!(
            "Failed to parse config file '{}': {}",
            args.config.display(),
            e
        )
    })?;

    tracing::info!(
        "Loaded station config from {}: {}",
        args.config.display(),
        station_config.station_id
    );

    // Create application state
    let app_state = StationState::new(station_config);

    // Build our application with routes
    let app = create_app(app_state);

    // Run our app with hyper
    let bind_addr = format!("0.0.0.0:{}", args.port);
    tracing::info!("Starting server on {}", bind_addr);

    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .map_err(|e| format!("Failed to bind to {}: {}", bind_addr, e))?;

    axum::serve(listener, app)
        .await
        .map_err(|e| format!("Server error: {}", e))?;

    Ok(())
}
