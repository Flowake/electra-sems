mod allocator;
mod models;

pub use crate::models::*;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SessionError {
    #[error("Connector {connector_id:?} is already in use by another session")]
    ConnectorAlreadyInUse { connector_id: ConnectorId },
    #[error("Connector {connector_id:?} does not exist in the station configuration")]
    ConnectorNotFound { connector_id: ConnectorId },
    #[error("Session {session_id} not found")]
    SessionNotFound { session_id: uuid::Uuid },
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

    pub fn get_config(&self) -> &StationConfig {
        &self.config
    }

    pub fn get_sessions(&self) -> &HashMap<uuid::Uuid, Session> {
        &self.sessions
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
        tracing::info!("Starting session for connector {}", connector_id);
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

        let new_session = allocator::allocate_for_new_session(
            self.sessions.clone(),
            &self.chargers,
            self.config.grid_capacity,
            self.charger_remaining_capacity(&connector_id.charger_id),
            &Session::new(connector_id, vehicle_max_power),
        );

        self.sessions
            .insert(new_session.session_id, new_session.clone());
        Ok(new_session)
    }

    pub fn stop_session(&mut self, session_id: uuid::Uuid) {
        tracing::info!("Stopping session {}", session_id);
        self.sessions.remove(&session_id);
    }

    /// If the consumed power is lower than the allocated power, then this
    /// will set this consumed power as the `vehicle_max_power` of the session,
    /// to free the power for other sessions.
    pub fn power_update(
        &mut self,
        session_id: uuid::Uuid,
        consumed_power: u32,
    ) -> Result<Session, SessionError> {
        tracing::info!("Updating power for session {}", session_id);
        let Some(mut previous_session) = self.sessions.get(&session_id).cloned() else {
            return Err(SessionError::SessionNotFound { session_id });
        };

        if consumed_power < previous_session.allocated_power {
            previous_session.vehicle_max_power = consumed_power;
        }

        let reallocated_session = allocator::allocate_for_new_session(
            self.sessions.clone(),
            &self.chargers,
            self.config.grid_capacity,
            self.charger_remaining_capacity(&previous_session.connector_id.charger_id)
                + previous_session.allocated_power,
            &previous_session,
        );

        self.sessions
            .insert(reallocated_session.session_id, reallocated_session.clone());

        Ok(reallocated_session)
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

    #[test]
    fn test_power_update_session_not_found() {
        let mut state = default_state();
        let non_existent_session_id = uuid::Uuid::new_v4();

        // Try to update power for a non-existent session
        let result = state.power_update(non_existent_session_id, 150);
        assert!(result.is_err());

        match result {
            Err(SessionError::SessionNotFound { session_id }) => {
                assert_eq!(session_id, non_existent_session_id);
            }
            _ => panic!("Expected SessionNotFound error"),
        }

        // Test successful power update
        let connector_id = ConnectorId {
            charger_id: "CP001".into(),
            idx: 1,
        };
        let session = state
            .start_session(connector_id, 100)
            .expect("Could not create session");

        let result = state.power_update(session.session_id, 80);
        assert!(result.is_ok());

        if let Ok(updated_session) = result {
            assert_eq!(updated_session.vehicle_max_power, 80);
        }
    }

    #[test]
    fn test_power_update() {
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

        // Then we update the power for the session_1, which should
        // have a lower max_power and allocated value
        let session_1 = state
            .power_update(session_1.session_id, 80)
            .expect("Error while updating power");
        assert_eq!(session_1.vehicle_max_power, 80);
        assert_eq!(session_1.allocated_power, 80);
    }

    #[test]
    fn test_power_update_full_charger() {
        let mut state = default_state();

        let session_1 = state
            .start_session(
                ConnectorId {
                    charger_id: "CP001".into(),
                    idx: 1,
                },
                200,
            )
            .expect("Could not create the session");

        assert_eq!(session_1.allocated_power, 200);

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

        assert_eq!(session_2.allocated_power, 0);

        // Then we update the power for the session_1, which consume
        // less than its allocated power.
        let session_1 = state
            .power_update(session_1.session_id, 80)
            .expect("Error while updating power");
        assert_eq!(session_1.vehicle_max_power, 80);
        assert_eq!(session_1.allocated_power, 80);

        // And we update session_2, which should receive more power
        // as some have been freed.
        let session_2 = state
            .power_update(session_2.session_id, 0)
            .expect("Error while updating power");
        assert_eq!(session_2.vehicle_max_power, 200);
        assert_eq!(session_2.allocated_power, 120);
    }

    #[test]
    fn test_power_update_accross_chargers() {
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

        // Lowering the power on the first session
        let session_1 = state
            .power_update(session_1.session_id, 80)
            .expect("Could not update power");
        assert_eq!(session_1.vehicle_max_power, 80);
        assert_eq!(session_1.allocated_power, 80);

        // Then session_3 update its power usage, and should receive more power
        let session_3 = state
            .power_update(session_3.session_id, 200)
            .expect("Could not update power");
        assert_eq!(session_3.vehicle_max_power, 300);
        assert_eq!(session_3.allocated_power, 200);
    }
}
