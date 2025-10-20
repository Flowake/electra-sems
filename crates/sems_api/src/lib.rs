//! SEMS API Library
//!
//! This library provides the HTTP API for the Station Energy Management System.

mod session;
mod station;

use axum::{
    Router,
    routing::{get, post},
};
use sems_core::StationState;
use std::sync::{Arc, Mutex};
use tower_http::trace::TraceLayer;

/// Health check endpoint
pub async fn health_check() -> &'static str {
    "OK"
}

/// Create the application router with all endpoints
pub fn create_app(app_state: StationState) -> Router {
    let shared_state = Arc::new(Mutex::new(app_state));
    Router::new()
        .route("/health", get(health_check))
        .route(
            "/station/config",
            get(station::get_station_config).post(station::update_station_config),
        )
        .route("/station/status", get(station::get_station_status))
        .route("/sessions", post(session::create_session))
        .route("/sessions/{session_id}/stop", post(session::stop_session))
        .route(
            "/sessions/{session_id}/power-update",
            post(session::power_update),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(shared_state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use sems_core::{ChargerConfig, ConnectorId, StationConfig};
    use tower::util::ServiceExt;

    pub fn create_test_app() -> Router {
        Router::new().route("/health", get(health_check))
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
    async fn test_health_endpoint() {
        let app = create_test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_integration_create_and_stop_session() {
        let config = test_station_config();
        let state = StationState::new(config);
        let app = create_app(state);

        // Create a session
        let create_request = session::CreateSessionRequest {
            connector_id: ConnectorId {
                charger_id: "CP001".to_string(),
                idx: 1,
            },
            vehicle_max_power: 150,
        };

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/sessions")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&create_request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let session_response: session::SessionResponse = serde_json::from_slice(&body).unwrap();
        let session_id = session_response.session.session_id;

        // Stop the session
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/sessions/{}/stop", session_id))
                    .method("POST")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_integration_session_with_power_update() {
        let config = test_station_config();
        let state = StationState::new(config);
        let app = create_app(state);

        // Create a session
        let create_request = session::CreateSessionRequest {
            connector_id: ConnectorId {
                charger_id: "CP001".to_string(),
                idx: 1,
            },
            vehicle_max_power: 150,
        };

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/sessions")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&create_request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let session_response: session::SessionResponse = serde_json::from_slice(&body).unwrap();
        let session_id = session_response.session.session_id;

        // Update power consumption
        let power_update_request = session::PowerUpdateRequest {
            consumed_power: 100,
        };

        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/sessions/{}/power-update", session_id))
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&power_update_request).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let updated_session_response: session::SessionResponse =
            serde_json::from_slice(&body).unwrap();

        assert_eq!(updated_session_response.session.session_id, session_id);
        assert_eq!(updated_session_response.session.vehicle_max_power, 100);
    }
}
