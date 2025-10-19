mod allocator;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

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

#[derive(Error, Debug)]
pub enum SessionError {
    #[error("Connector {connector_id:?} is already in use by another session")]
    ConnectorAlreadyInUse { connector_id: ConnectorId },
    #[error("Connector {connector_id:?} does not exist in the station configuration")]
    ConnectorNotFound { connector_id: ConnectorId },
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

    pub fn start_session(
        &mut self,
        connector_id: ConnectorId,
        vehicle_max_power: u32,
    ) -> Result<Session, SessionError> {
        // Check if the connector exists in the station configuration
        if let Some(charger) = self.chargers.get(&connector_id.charger_id) {
            if connector_id.idx == 0 || connector_id.idx > charger.connectors {
                return Err(SessionError::ConnectorNotFound {
                    connector_id: connector_id.clone(),
                });
            }
        } else {
            return Err(SessionError::ConnectorNotFound {
                connector_id: connector_id.clone(),
            });
        }

        // Check if the connector is already in use
        if self
            .sessions
            .values()
            .any(|session| session.connector_id == connector_id)
        {
            return Err(SessionError::ConnectorAlreadyInUse { connector_id });
        }
        let station_remaining_capacity = self.station_remaining_capacity();
        let charger_remaining_capacity = self.charger_remaining_capacity(&connector_id.charger_id);

        // The session cannot exceed this capacity
        let hardcap_capacity = station_remaining_capacity.min(charger_remaining_capacity);

        let new_session = Session::new(connector_id, vehicle_max_power);
        let mut current_sessions = self.sessions.clone();
        current_sessions.insert(new_session.session_id, new_session.clone());

        let mut reallocated_sessions = allocator::allocate_power_station(
            &current_sessions,
            &self.chargers,
            self.config.grid_capacity,
        );
        let mut allocated_session = reallocated_sessions
            .remove_entry(&new_session.session_id)
            .expect("Could not find allocated session")
            .1;

        // The reallocation might lower the power for other sessions, but it will not be effective
        // immediately (not until their next call to power_update). As such we need to ensure that
        // we do not exceed the capacity.
        allocated_session.allocated_power = allocated_session.allocated_power.min(hardcap_capacity);
        self.sessions
            .insert(new_session.session_id, allocated_session.clone());
        Ok(allocated_session)
    }

    pub fn stop_session(&mut self, session_id: uuid::Uuid) {
        self.sessions.remove(&session_id);
    }

    pub fn power_update(&mut self, session_id: uuid::Uuid, power: u32) -> Session {
        todo!()
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

    #[test]
    fn test_connector_already_in_use() {
        let mut state = default_state();
        let connector_id = ConnectorId {
            charger_id: "CP001".into(),
            idx: 1,
        };

        // First session should succeed
        let result1 = state.start_session(connector_id.clone(), 100);
        assert!(result1.is_ok());

        // Second session on same connector should fail
        let result2 = state.start_session(connector_id.clone(), 50);
        assert!(result2.is_err());

        match result2 {
            Err(SessionError::ConnectorAlreadyInUse {
                connector_id: err_connector_id,
            }) => {
                assert_eq!(err_connector_id, connector_id);
            }
            _ => panic!("Expected ConnectorAlreadyInUse error"),
        }

        // Different connector should still work
        let different_connector = ConnectorId {
            charger_id: "CP001".into(),
            idx: 2,
        };
        let result3 = state.start_session(different_connector, 75);
        assert!(result3.is_ok());
    }

    #[test]
    fn test_connector_not_found() {
        let mut state = default_state();

        // Test non-existent charger
        let invalid_charger = ConnectorId {
            charger_id: "INVALID".into(),
            idx: 1,
        };

        let result = state.start_session(invalid_charger.clone(), 100);
        assert!(result.is_err());

        match result {
            Err(SessionError::ConnectorNotFound {
                connector_id: err_connector_id,
            }) => {
                assert_eq!(err_connector_id, invalid_charger);
            }
            _ => panic!("Expected ConnectorNotFound error"),
        }

        // Test invalid connector index (0)
        let invalid_idx_zero = ConnectorId {
            charger_id: "CP001".into(),
            idx: 0,
        };

        let result = state.start_session(invalid_idx_zero.clone(), 100);
        assert!(result.is_err());

        match result {
            Err(SessionError::ConnectorNotFound {
                connector_id: err_connector_id,
            }) => {
                assert_eq!(err_connector_id, invalid_idx_zero);
            }
            _ => panic!("Expected ConnectorNotFound error"),
        }

        // Test invalid connector index (too high)
        let invalid_idx_high = ConnectorId {
            charger_id: "CP001".into(),
            idx: 5, // CP001 only has 2 connectors
        };

        let result = state.start_session(invalid_idx_high.clone(), 100);
        assert!(result.is_err());

        match result {
            Err(SessionError::ConnectorNotFound {
                connector_id: err_connector_id,
            }) => {
                assert_eq!(err_connector_id, invalid_idx_high);
            }
            _ => panic!("Expected ConnectorNotFound error"),
        }

        // Test valid connector should work
        let valid_connector = ConnectorId {
            charger_id: "CP001".into(),
            idx: 1,
        };

        let result = state.start_session(valid_connector, 100);
        assert!(result.is_ok());
    }

    #[test]
    fn test_start_session() {
        let mut state = default_state();

        let session_1 = state
            .start_session(
                ConnectorId {
                    charger_id: "CP001".into(),
                    idx: 1,
                },
                100,
            )
            .expect("Could not create the session");

        assert_eq!(session_1.allocated_power, 100);

        // Starting a session with a greater max_power
        // hitting the charger capacity
        let session_2 = state
            .start_session(
                ConnectorId {
                    charger_id: "CP001".into(),
                    idx: 2,
                },
                200,
            )
            .expect("Could not create the session");

        assert_eq!(session_2.allocated_power, 100);

        // Starting a session on another charger,
        // reaching grid capacity
        let session_3 = state
            .start_session(
                ConnectorId {
                    charger_id: "CP003".into(),
                    idx: 1,
                },
                300,
            )
            .expect("Could not create the session");
        assert_eq!(session_3.allocated_power, 200);

        // Starting a session on a new charger,
        // but no capacity left
        let session_4 = state
            .start_session(
                ConnectorId {
                    charger_id: "CP002".into(),
                    idx: 1,
                },
                200,
            )
            .expect("Could not create the session");
        assert_eq!(session_4.allocated_power, 0);

        // Removing a session to free some capacity and adding a new session
        // that will receive its fair share.
        state.stop_session(session_3.session_id);

        let session_5 = state
            .start_session(
                ConnectorId {
                    charger_id: "CP002".into(),
                    idx: 2,
                },
                200,
            )
            .expect("Could not create the session");

        assert_eq!(session_5.allocated_power, 100);
    }
}
