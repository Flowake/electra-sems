use crate::config::StationConfig;

/// Application state that contains the station configuration and other shared state
#[derive(Debug, Clone)]
pub struct AppState {
    /// The station configuration loaded at startup (immutable)
    pub station_config: StationConfig,
}

impl AppState {
    /// Creates a new AppState with the given station configuration
    pub fn new(station_config: StationConfig) -> Self {
        Self { station_config }
    }

    /// Gets a reference to the station configuration
    pub fn get_station_config(&self) -> &StationConfig {
        &self.station_config
    }
}
