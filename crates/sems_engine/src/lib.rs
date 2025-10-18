use sems_core::{ConnectorId, Session, StationConfig, StationState};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartSession {
    connector_id: ConnectorId,
    vehicle_max_power: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartSessionResponse {
    session_id: String,
    allocated_power: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopSession {
    session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopSessionResponse {
    session_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSessionPower {
    session_id: String,
    power: u32,
}

pub struct Engine {
    station_state: StationState,
}

impl Engine {
    pub fn new(station_config: StationConfig) -> Self {
        Engine {
            station_state: StationState::new(station_config),
        }
    }

    pub fn start_session(&mut self, start_session: StartSession) -> Session {
        self.station_state
            .start_session(start_session.connector_id, start_session.vehicle_max_power)
    }

    pub fn stop_session(&mut self, session_id: uuid::Uuid) {
        self.station_state.stop_session(session_id);
    }
}
