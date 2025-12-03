use anyhow::Result;
use monitoring::{MonitoringHub, MonitoringLayer};
use recording::RecordingState;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, Mutex};
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::filter as other_filter;
use tracing_subscriber::fmt::time::ChronoLocal;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

mod alerts;
mod audio;
mod backend;
mod cleanup;
mod config;
mod filter;
mod header;
mod monitoring;
mod recording;
mod relay;
mod state;
mod webhook;

use config::Config;
use state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::from_config_json("/app/config.json")?;

    let monitoring = MonitoringHub::new(
        config.monitoring_max_log_entries,
        Duration::from_secs(config.monitoring_activity_window_secs),
    );

    let timer = ChronoLocal::new("%Y-%m-%d %I:%M:%S.%3f %p ".to_string());
    let file_appender =
        tracing_appender::rolling::daily(&config.shared_state_dir, &config.alert_log_file);
    let (non_blocking_file, _guard) = tracing_appender::non_blocking(file_appender);
    let env_filter = EnvFilter::from_default_env();
    let log_level = config
        .log_level
        .parse::<LevelFilter>()
        .unwrap_or(LevelFilter::INFO);
    let monitoring_layer = MonitoringLayer::new(monitoring.clone());
    let filter = other_filter::Targets::new()
        .with_default(log_level)
        .with_target("symphonia", tracing::Level::ERROR);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking_file)
                .with_ansi(false)
                .with_timer(timer.clone()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stdout)
                .with_timer(timer),
        )
        .with(monitoring_layer)
        .with(filter)
        .init();

    info!("Starting EAS Listener...");

    let app_state = Arc::new(Mutex::new(AppState::new(config.filters.clone())));
    let recording_state = Arc::new(Mutex::new(Option::<RecordingState>::None));

    let (tx, rx) = mpsc::channel::<(String, String, String, String, Duration, String)>(32);
    let (nnnn_tx, _nnnn_rx) = broadcast::channel::<()>(1);

    let audio_processor_handle = tokio::spawn(audio::run_audio_processor(
        config.clone(),
        tx,
        recording_state.clone(),
        nnnn_tx.clone(),
        monitoring.clone(),
    ));
    let alert_manager_handle = tokio::spawn(alerts::run_alert_manager(
        config.clone(),
        app_state.clone(),
        rx,
        recording_state,
        nnnn_tx.subscribe(),
        monitoring.clone(),
    ));
    let state_cleanup_handle = tokio::spawn(alerts::run_state_cleanup(
        config.clone(),
        app_state.clone(),
        monitoring.clone(),
    ));
    let log_cleanup_handle = tokio::spawn(cleanup::run_log_cleanup(config.clone()));
    let api_handle = tokio::spawn(backend::run_server(
        config.monitoring_bind_addr,
        app_state.clone(),
        monitoring,
    ));

    tokio::select! {
        _ = audio_processor_handle => info!("Audio processor task exited."),
        _ = alert_manager_handle => info!("Alert manager task exited."),
        _ = state_cleanup_handle => info!("State cleanup task exited."),
        _ = log_cleanup_handle => info!("Log cleanup task exited."),
        _ = api_handle => info!("Monitoring API task exited."),
    };

    Ok(())
}
