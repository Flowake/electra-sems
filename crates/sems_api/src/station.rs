use axum::{Json, extract::State};
use sems_core::{Session, StationConfig, StationState};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StationStatus {
    pub sessions: HashMap<uuid::Uuid, Session>,
}

/// Get current station configuration
pub async fn get_station_config(
    State(app_state): State<Arc<Mutex<StationState>>>,
) -> Json<StationConfig> {
    tracing::info!("Getting station configuration");
    let state = app_state.lock().unwrap();
    let config = state.get_config().clone();
    Json(config)
}

/// Get station status with all current sessions
pub async fn get_station_status(
    State(app_state): State<Arc<Mutex<StationState>>>,
) -> Json<StationStatus> {
    tracing::info!("Getting station status");
    let state = app_state.lock().unwrap();
    let sessions = state.get_sessions().clone();
    Json(StationStatus { sessions })
}

#[cfg(test)]
mod tests {
    use super::*;
    pub use axum::{Router, routing::get};
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::util::ServiceExt;

    use sems_core::ChargerConfig;

    /// Create the application router with all endpoints
    pub fn create_app(app_state: StationState) -> Router {
        let shared_state = Arc::new(Mutex::new(app_state));
        Router::new()
            .route("/station/config", get(get_station_config))
            .route("/station/status", get(get_station_status))
            .with_state(shared_state)
    }

    fn test_station_config() -> StationConfig {
        StationConfig {
            station_id: "TEST_STATION".into(),
            grid_capacity: 400,
            chargers: vec![ChargerConfig {
                id: "CP001".into(),
                max_power: 200,
                connectors: 2,
            }],
            battery: None,
        }
    }

    #[tokio::test]
    async fn test_config_endpoint() {
        let config = test_station_config();
        let expected_station_id = config.station_id.clone();
        let state = StationState::new(config);
        let app = create_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/station/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let config_response: StationConfig = serde_json::from_slice(&body).unwrap();
        assert_eq!(config_response.station_id, expected_station_id);
    }

    #[tokio::test]
    async fn test_station_status_endpoint_empty() {
        let config = test_station_config();
        let state = StationState::new(config);
        let app = create_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/station/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let status_response: StationStatus = serde_json::from_slice(&body).unwrap();
        assert!(status_response.sessions.is_empty());
    }

    #[tokio::test]
    async fn test_station_status_endpoint_with_sessions() {
        use sems_core::ConnectorId;

        let config = test_station_config();
        let mut state = StationState::new(config);

        // Start a session
        let connector_id = ConnectorId {
            charger_id: "CP001".to_string(),
            idx: 1,
        };
        let session_result = state.start_session(connector_id, 150).unwrap();
        let session_id = session_result.session_id;

        let app = create_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/station/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let status_response: StationStatus = serde_json::from_slice(&body).unwrap();

        // Verify we have one session
        assert_eq!(status_response.sessions.len(), 1);
        assert!(status_response.sessions.contains_key(&session_id));

        // Verify session details
        let session = &status_response.sessions[&session_id];
        assert_eq!(session.session_id, session_id);
        assert_eq!(session.connector_id.charger_id, "CP001");
        assert_eq!(session.connector_id.idx, 1);
        assert_eq!(session.vehicle_max_power, 150);
    }
}
