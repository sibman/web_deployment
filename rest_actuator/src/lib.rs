pub mod api {
use axum::{routing::get, body::Body, http::{Response, StatusCode}, Router,};
use std::{sync::{Arc, Mutex}, collections::HashMap};
use axum::response::IntoResponse;
use serde_json::json;
use std::fmt::Debug;

// Define a trait for health checkers
pub trait HealthChecker: Send + Sync + Debug {
    fn is_ready(&self) -> bool;
    fn is_alive(&self) -> bool;
}

// Actuator struct to manage health checkers and routes
pub struct Actuator {
    health_checkers: Arc<HashMap<String, Arc<Mutex<dyn HealthChecker>>>>,
}

impl Actuator {
    // Create a new Actuator instance
    pub fn new() -> Self {
        Self {
            health_checkers: Arc::new(HashMap::new()),
        }
    }

    // Add a health checker
    pub fn add_health_checker(&mut self, name: String, checker: Arc<Mutex<dyn HealthChecker>>) {
        let health_checkers = Arc::get_mut(&mut self.health_checkers).unwrap();
        health_checkers.insert(name, checker);
    }

    // Generate the actuator router
    pub fn router(&self, router: Router) -> Router {
        let health_checkers_readiness = self.health_checkers.clone();
        let health_checkers_liveness = self.health_checkers.clone();
        let health_checkers_info = self.health_checkers.clone();
        let health_checkers_health = self.health_checkers.clone();
        // Create a router with /actuator/health/readiness, /actuator/health/liveness, /actuator/info, and /actuator/health endpoints
        router
            .route("/actuator/health/readiness", get(|| async move {                
                readiness_handler(health_checkers_readiness).await // Call the readiness_handler method within the closure
            }))
            .route("/actuator/health/liveness", get(|| async move {
                liveness_handler(health_checkers_liveness).await // Call the liveness_handler method within the closure
            }))
            .route("/actuator/info", get(|| async move {
                info_handler(health_checkers_info).await // Call the info_handler method within the closure
            }))
            .route("/actuator/health", get(|| async move {
                health_handler(health_checkers_health).await // Call the health_handler method within the closure
            }))
    }
}

// Handler for /actuator/info endpoint
async fn info_handler(health_checkers: Arc<HashMap<String, Arc<Mutex<dyn HealthChecker>>>>) -> impl IntoResponse {
    let is_ready = check_all_health(health_checkers.clone(), |checker| checker.is_ready()).await;
    let is_alive = check_all_health(health_checkers, |checker| checker.is_alive()).await;

    Response::builder()
        .status(if is_ready && is_alive { StatusCode::OK } else { StatusCode::CONFLICT })
        .header("Content-Type", "application/json")
        .body(Body::empty())
        .unwrap()
}

// Placeholder health handler function
async fn health_handler(health_checkers: Arc<HashMap<String, Arc<Mutex<dyn HealthChecker>>>>) -> impl IntoResponse {
    let is_ready = check_all_health(health_checkers.clone(), |checker| checker.is_ready()).await;
    let is_alive = check_all_health(health_checkers, |checker| checker.is_alive()).await;

    let status = if is_ready && is_alive {
        "UP"
    } else {
        "DOWN"
    };

    Response::builder()
        .status(if is_ready && is_alive { StatusCode::OK } else { StatusCode::CONFLICT })
        .header("Content-Type", "application/json")
        .body(json!({ "status": status }).to_string())
        .unwrap()
}

// Handler for /actuator/health/readiness endpoint
async fn readiness_handler(health_checkers: Arc<HashMap<String, Arc<Mutex<dyn HealthChecker>>>>) -> impl IntoResponse {
    let is_ready = check_all_health(health_checkers, |checker| checker.is_ready()).await;

    Response::builder()
        .status(if is_ready { StatusCode::OK } else { StatusCode::CONFLICT })
        .header("Content-Type", "application/json")
        .body(json!({ "status": if is_ready { "UP" } else { "DOWN" } }).to_string())
        .unwrap()
}

// Handler for /actuator/health/liveness endpoint
async fn liveness_handler(health_checkers: Arc<HashMap<String, Arc<Mutex<dyn HealthChecker>>>>) -> impl IntoResponse {
    let is_alive = check_all_health(health_checkers, |checker| checker.is_alive()).await;

    Response::builder()
        .status(if is_alive { StatusCode::OK } else { StatusCode::CONFLICT })
        .header("Content-Type", "application/json")
        .body(json!({ "status": if is_alive { "UP" } else { "DOWN" } }).to_string())
        .unwrap()
}

// Helper function to check all health checkers
async fn check_all_health<F>(health_checkers: Arc<HashMap<String, Arc<Mutex<dyn HealthChecker>>>>, check_fn: F) -> bool
where
    F: Fn(&dyn HealthChecker) -> bool,
{
    let mut is_health = true;
    for (_, checker) in health_checkers.iter() {
        let checker = checker.lock().unwrap();
        if !check_fn(&*checker) {
            is_health = false;
            break;
        }
    }
    is_health
}
}

// Example health check function
// pub fn database_health_check() -> bool {
//     // Placeholder implementation for database health check
//     true
// }

// fn main() {
//     // Create a new Actuator instance
//     let mut actuator = Actuator::new();

//     // Add health checkers
//     actuator.add_health_checker("database".to_string(), Arc::new(Mutex::new(database_health_check)));

//     // Generate the actuator router
//     let router = actuator.router();

//     // Start the server with the actuator routes
//     // axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
//     //     .serve(router.into_make_service())
//     //     .await
//     //     .unwrap();
// }
#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        extract::connect_info::MockConnectInfo,
        http::{self, Request, StatusCode},
    };
    use http_body_util::BodyExt; // for `collect`
    use serde_json::{json, Value};
    use std::net::SocketAddr;
    use tokio::net::TcpListener;
    use tower::{Service, ServiceExt}; // for `call`, `oneshot`, and `ready`
    use http::Method;
    use std::{sync::{Arc, Mutex}, collections::HashMap};
    use api::HealthChecker;
    use axum::extract::ConnectInfo;
    use axum::routing::get;
    use axum::Json;
    use axum::routing::post;
    use axum::Router;

    pub fn app() -> Router {
        // Compose the routes
        Router::new()
            .route(
                "/json",
                post(|payload: Json<serde_json::Value>| async move {
                    Json(serde_json::json!({ "data": payload.0 }))
                }),
            )
            .route(
                "/requires-connect-info",
                get(|ConnectInfo(addr): ConnectInfo<SocketAddr>| async move { format!("Hi {addr}") }),
            )
            // Add middleware to all routes
            // .layer(
            // )
    }

    #[tokio::test]
    async fn json() {
        let app = app();

        let response = app
            .oneshot(
                Request::builder()
                    .method(http::Method::POST)
                    .uri("/json")
                    .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                    .body(Body::from(
                        serde_json::to_vec(&json!([1, 2, 3, 4])).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body, json!({ "data": [1, 2, 3, 4] }));
    }

    // Here we're calling `/requires-connect-info` which requires `ConnectInfo`
    //
    // That is normally set with `Router::into_make_service_with_connect_info` but we can't easily
    // use that during tests. The solution is instead to set the `MockConnectInfo` layer during
    // tests.
    #[tokio::test]
    async fn with_into_make_service_with_connect_info() {
        let mut app = app()
            .layer(MockConnectInfo(SocketAddr::from(([0, 0, 0, 0], 3000))))
            .into_service();

        let request = Request::builder()
            .uri("/requires-connect-info")
            .body(Body::empty())
            .unwrap();
        let response = app.ready().await.unwrap().call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[derive(Debug)]
    struct DatabaseHealthCheck{
        ready: bool,
        alive: bool
    }
    
    impl HealthChecker for DatabaseHealthCheck {
        fn is_ready(&self) -> bool {
            self.ready
        }

        fn is_alive(&self) -> bool {
            self.alive
        }
    }

    #[tokio::test]
    async fn inject_actuator() {
        let mut app = app();
        // Create a new Actuator instance
        let mut actuator = api::Actuator::new();

        // Add health checkers
        actuator.add_health_checker("database".to_string(), Arc::new(Mutex::new(DatabaseHealthCheck{ready: true, alive: true})));

        // Generate the actuator router
        let mut app = actuator.router(app)     
            .into_service();

        let request = Request::builder()
            .method(Method::GET)
            .uri("/actuator/health")
            .body(Body::empty())
            .unwrap();
        
        let response = app.ready().await.unwrap().call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = Request::builder()
            .method(Method::GET)
            .uri("/actuator/info")
            .body(Body::empty())
            .unwrap();
        
        let response = app.ready().await.unwrap().call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = Request::builder()
            .method(Method::GET)
            .uri("/actuator/health/liveness")
            .body(Body::empty())
            .unwrap();
        
        let response = app.ready().await.unwrap().call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = Request::builder()
            .method(Method::GET)
            .uri("/actuator/health/readiness")
            .body(Body::empty())
            .unwrap();
        
        let response = app.ready().await.unwrap().call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
