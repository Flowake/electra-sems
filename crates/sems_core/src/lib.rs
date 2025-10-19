mod allocator;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StationConfig {
    station_id: String,
    grid_capacity: u32,
    chargers: Vec<ChargerConfig>,
    battery: Option<Bess>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChargerConfig {
    id: String,
    max_power: u32,
    connectors: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bess {
    initial_capacity: u32,
    power: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    session_id: uuid::Uuid,
    connector_id: ConnectorId,
    allocated_power: u32,
    vehicle_max_power: u32,
}

impl Session {
    fn new(connector_id: ConnectorId, vehicle_max_power: u32) -> Self {
        Session {
            session_id: uuid::Uuid::new_v4(),
            connector_id,
            allocated_power: 0,
            vehicle_max_power,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorId {
    pub charger_id: String,
    idx: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StationState {
    config: StationConfig,
    sessions: HashMap<uuid::Uuid, Session>,
    chargers: HashMap<String, ChargerConfig>,
}

impl StationState {
    pub fn new(config: StationConfig) -> Self {
        let chargers = config
            .chargers
            .iter()
            .map(|charger| (charger.id.clone(), charger.clone()))
            .collect();
        StationState {
            config,
            chargers,
            sessions: HashMap::new(),
        }
    }

    /// Return the total allocated power of the station.
    ///
    /// This is the sum of all allocated power of all sessions.
    fn station_allocated_power(&self) -> u32 {
        self.sessions
            .values()
            .map(|session| session.allocated_power)
            .sum()
    }

    /// Return the remaining capacity of the station.
    ///
    /// This is the difference between the grid capacity and the total allocated power.
    fn station_remaining_capacity(&self) -> u32 {
        self.config.grid_capacity - self.station_allocated_power()
    }

    /// Return the remaining capacity of a charger.
    ///
    /// This is the difference between the maximum power of the charger and the total allocated power of all sessions connected to the charger.
    ///
    /// Note: This cannot exceed the remaining capacity of the station.
    fn charger_remaining_capacity(&self, charger_id: &str) -> u32 {
        let station_remaining_capacity = self.station_remaining_capacity();
        self.chargers
            .get(charger_id)
            .map_or(0, |charger| {
                charger.max_power
                    - self
                        .sessions
                        .values()
                        .filter(|session| session.connector_id.charger_id == charger_id)
                        .map(|session| session.allocated_power)
                        .sum::<u32>()
            })
            .min(station_remaining_capacity)
    }

    pub fn start_session(&mut self, connector_id: ConnectorId, vehicle_max_power: u32) -> Session {
        let new_session = Session::new(connector_id, vehicle_max_power);
        self.sessions
            .insert(new_session.session_id, new_session.clone());
        new_session
    }

    pub fn stop_session(&mut self, session_id: uuid::Uuid) {
        self.sessions.remove(&session_id);
    }

    /// Update the power allocation for a session.
    ///
    /// The allocated power is updated to the new value, while ensuring
    /// that the power allocation does not exceed the remaining capacity of the charger
    /// and the station's total capacity.
    pub fn power_update(&mut self, session_id: uuid::Uuid, power: u32) -> Session {
        let new_allocated_power = {
            let session = self.sessions.get(&session_id).expect("Session not found");

            power.min(self.charger_remaining_capacity(&session.connector_id.charger_id))
        };
        let session = self
            .sessions
            .get_mut(&session_id)
            .expect("Session not found");
        session.allocated_power = new_allocated_power;
        session.clone()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn default_config() -> StationConfig {
        StationConfig {
            station_id: "ELECTRA_PARIS_15".into(),
            grid_capacity: 400,
            chargers: vec![
                ChargerConfig {
                    id: "CP001".into(),
                    max_power: 200,
                    connectors: 2,
                },
                ChargerConfig {
                    id: "CP002".into(),
                    max_power: 200,
                    connectors: 2,
                },
                ChargerConfig {
                    id: "CP003".into(),
                    max_power: 300,
                    connectors: 2,
                },
            ],
            battery: None,
        }
    }

    fn default_state() -> StationState {
        StationState::new(default_config())
    }
}
