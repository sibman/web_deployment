pub mod api {
    use axum::extract::Extension;
    use axum::response::IntoResponse;
    use axum::{
        body::Body,
        http::{Response, StatusCode},
        routing::{get},
        Router,
    };
    use serde_json::json;
    use std::fmt::Debug;
    use std::time::Duration;
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };
    use tokio::sync::broadcast;
    
    //Handler for /actuator/info endpoint
    pub async fn info_handler(Extension(state): Extension<ActuatorState>) -> impl IntoResponse {
        let is_ready = state.is_ready && check_all_health(&state.health_checkers, |checker| checker.is_ready()).await;
        let is_alive = state.is_alive && check_all_health(&state.health_checkers, |checker| checker.is_alive()).await;

        Response::builder()
            .status(if is_ready && is_alive {
                StatusCode::OK
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            })
            .header("Content-Type", "application/json")
            .body(Body::empty())
            .unwrap()
    }

    // Placeholder health handler function
    pub async fn health_handler(Extension(state): Extension<ActuatorState>) -> impl IntoResponse {
        let is_ready = state.is_ready && check_all_health(&state.health_checkers, |checker| checker.is_ready()).await;
        let is_alive = state.is_alive && check_all_health(&state.health_checkers, |checker| checker.is_alive()).await;
        let status = if is_ready && is_alive { "UP" } else { "DOWN" };

        Response::builder()
            .status(if is_ready && is_alive {
                StatusCode::OK
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            })
            .header("Content-Type", "application/json")
            .body(json!({ "status": status }).to_string())
            .unwrap()
    }

    // Handler for /actuator/health/readiness endpoint
    pub async fn readiness_handler(Extension(state): Extension<ActuatorState>) -> impl IntoResponse {
        let is_ready = state.is_ready && check_all_health(&state.health_checkers, |checker| checker.is_ready()).await;
        let body = json!({ "status": if is_ready { "UP" } else { "DOWN" } });

        Response::builder()
            .status(if is_ready {
                StatusCode::OK
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            })
            .body(body.to_string())
            .unwrap()
    }

    // Handler for /actuator/health/liveness endpoint
    pub async fn liveness_handler(Extension(state): Extension<ActuatorState>) -> impl IntoResponse {
        let is_alive = state.is_alive && check_all_health(&state.health_checkers, |checker| checker.is_alive()).await;
        let body = json!({ "status": if is_alive { "UP" } else { "DOWN" } });

        Response::builder()
            .status(if is_alive {
                StatusCode::OK
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            })
            .body(body.to_string())
            .unwrap()
    }

    async fn check_all_health<F>(health_checkers: &ActuatorStateDb, check_fn: F) -> bool
    where
        F: Fn(&dyn StateChecker) -> bool,
    {
        let mut is_health = true;
        for (_, checker) in health_checkers.iter() {
            let checker = checker.lock().unwrap();
            if !check_fn(&**checker) {
                is_health = false;
                break;
            }
        }
        is_health
    }

    // Define a trait for health checkers
    pub trait StateChecker: Send + Sync + Debug {
        fn is_ready(&self) -> bool;
        fn is_alive(&self) -> bool;
    }

    type ActuatorStateDb = Arc<HashMap<String, Arc<Mutex<Box<dyn StateChecker>>>>>;

    // ActuatorState struct to manage health checkers and routes
    #[derive(Debug, Clone)]
    pub struct ActuatorState {
        health_checkers: ActuatorStateDb,
        state_check_sender: broadcast::Sender<()>,
        state_check_receiver: Arc<Mutex<broadcast::Receiver<()>>>,
        is_ready: bool,
        is_alive: bool,
        is_health: bool,
    }

    impl ActuatorState {
        // Create a new ActuatorState instance
        pub fn new() -> Self {
            let (state_check_sender, state_check_receiver) = broadcast::channel::<()>(1);
            let state_clone_sender = state_check_sender.clone(); // Clone the sender
            let state_clone_receiver = Arc::new(Mutex::new(state_check_receiver));

            let state = Self {
                health_checkers: Arc::new(HashMap::new()),
                state_check_sender,
                state_check_receiver: state_clone_receiver.clone(),
                is_ready: true,
                is_alive: true,
                is_health: true,
            };

            let mut state_clone = state.clone();
            tokio::spawn(async move {
                let state_clone_receiver = state_clone_sender.subscribe();
                state_clone.state_check_loop(state_clone_receiver).await;
            });

            state
        }

        async fn state_check_loop(&mut self, mut receiver: broadcast::Receiver<()>) {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            
            loop {
                // Check for messages on the receiver alongside the interval
                tokio::select! {
                    _ = interval.tick() => {
                        // Scheduled check
                        self.check_all_health().await;
                    }
                    _ = receiver.recv() => {
                        // Manual check triggered via sender
                        self.check_all_health().await;
                    }
                }
            }
        }

        async fn check_all_health(&mut self) {
            let mut new_check = true;
            self.is_health = true;

            for (_, checker) in self.health_checkers.iter() {
                let checker = checker.lock().unwrap();
                let is_ready = checker.is_ready();
                let is_alive = checker.is_alive();

                if new_check && !is_alive {
                    self.is_alive = is_alive;
                    self.is_health = is_alive;
                    return; // Early return if unhealthy
                } else if new_check && !is_ready {
                    self.is_ready = is_ready;
                    self.is_health = is_ready;
                    return; // Early return if unhealthy
                }

                new_check = self.is_health; // Update new_check only if still healthy
            }

            // If loop finishes without early return, set to healthy
            self.is_ready = true;
            self.is_alive = true;
        }

        // Trigger state check manually
        pub fn trigger_state_check(&self) {
            let _ = self.state_check_sender.send(());
        }

        // create state check receiver manually
        pub fn create_state_check_receiver(&self) -> Arc<Mutex<broadcast::Receiver<()>>> {
            self.state_check_receiver.clone()
        }

        // Add a health checker
        pub fn add_health_checker(
            &mut self,
            name: String,
            checker: Arc<Mutex<Box<dyn StateChecker>>>,
        ) {
            if let Some(health_checkers) = Arc::get_mut(&mut self.health_checkers) {
                health_checkers.insert(name, checker);
                println!("{:?}", health_checkers);
            } else {
                // Handle the case where the value is None
                println!("Health check value is not available");
            }
        }
    }

    #[derive(Debug)]
    pub struct ActuatorRouterBuilder {
        router: Router,
    }

    impl ActuatorRouterBuilder {
        pub fn new(router: Router) -> Self {
            Self {
                router,
            }
        }

        pub fn with_layer<T: Clone + Send + Sync + 'static>(mut self, extention_opt: Option<Extension<T>>) -> Self { //ActuatorState
            if let Some(extention) = extention_opt {
                self.router = self.router.layer(extention);
            }
            self
        }

        //TODO: need to figure out how to create route dinamically
        // pub fn with_route<H, S>(mut self, uri: &str, handler: H) -> Self 
        // where
        //     H: Handler<(), Extension<S>>,
        //     S: Clone + Send + Sync + 'static, {
        //     self.router = self.router.route(uri, get(handler));
        //     self
        // }

        pub fn with_readiness_route(mut self) -> Self {
            self.router = self.router.route("/actuator/health/readiness", get(readiness_handler));
            self
        }

        pub fn with_liveness_route(mut self) -> Self {
            self.router = self.router.route("/actuator/health/liveness", get(liveness_handler));
            self
        }

        pub fn with_info_route(mut self) -> Self {
            self.router = self.router.route("/actuator/info", get(info_handler));
            self
        }

        pub fn with_health_route(mut self) -> Self {
            self.router = self.router.route("/actuator/health", get(health_handler));
            self
        }

        pub fn build(self) -> Router {
            self.router
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        extract::connect_info::MockConnectInfo,
        extract::{ConnectInfo, Extension},
        http::{self, Request, StatusCode},
        routing::{get, post},
        Json, Router,
    };
    use http_body_util::BodyExt; // for `collect`
    use serde_json::{json, Value};
    use std::net::SocketAddr;

    use api::{ActuatorRouterBuilder, ActuatorState, StateChecker}; 
    use http::Method;
    use std::sync::{Arc, Mutex};
    use tower::{Service, ServiceExt}; // for `call`, `oneshot`, and `ready`

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
    struct DatabaseHealthCheck {
        ready: bool,
        alive: bool,
    }

    impl StateChecker for DatabaseHealthCheck {
        fn is_ready(&self) -> bool {
            self.ready
        }

        fn is_alive(&self) -> bool {
            self.alive
        }
    }

    #[tokio::test]
    async fn test_actuator() {
        let _app = app();
        let mut actuator_state = api::ActuatorState::new();

        // Add health checkers
        actuator_state.add_health_checker(
            "database".to_string(),
            Arc::new(Mutex::new(Box::new(DatabaseHealthCheck {
                ready: true,
                alive: true,
            }))),
        );

        println!("{:?}", actuator_state);

        actuator_state.add_health_checker(
            "database".to_string(),
            Arc::new(Mutex::new(Box::new(DatabaseHealthCheck {
                ready: false,
                alive: false,
            }))),
        );

        println!("{:?}", actuator_state);

        actuator_state.add_health_checker(
            "database".to_string(),
            Arc::new(Mutex::new(Box::new(DatabaseHealthCheck {
                ready: true,
                alive: true,
            }))),
        );

        println!("{:?}", actuator_state);
    }

    #[tokio::test]
    async fn inject_actuator() {
        let app = app();
        // Create a new ActuatorState instance
        let mut actuator_state = api::ActuatorState::new();

        // Add health checkers
        actuator_state.add_health_checker(
            "database".to_string(),
            Arc::new(Mutex::new(Box::new(DatabaseHealthCheck {
                ready: true,
                alive: true,
            }))),
        );

        let extention: Option<Extension<ActuatorState>> = Some(Extension(actuator_state));
        
        let mut app = ActuatorRouterBuilder::new(app)
            .with_readiness_route()
            .with_liveness_route()
            .with_info_route()
            .with_health_route()
            .with_layer(extention)
            .build()
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

        //TODO: Figure out howto test state chages, consider mockup
        // Add health checkers
        // actuator.add_health_checker("database".to_string(), Arc::new(Mutex::new(DatabaseHealthCheck{ready: false, alive: false})));

        // println!("{:?}", actuator);

        // let request = Request::builder()
        // .method(Method::GET)
        // .uri("/actuator/health")
        // .body(Body::empty())
        // .unwrap();

        // let response = app.ready().await.unwrap().call(request).await.unwrap();
        // assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        // let request = Request::builder()
        //     .method(Method::GET)
        //     .uri("/actuator/info")
        //     .body(Body::empty())
        //     .unwrap();

        // let response = app.ready().await.unwrap().call(request).await.unwrap();
        // assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        // let request = Request::builder()
        //     .method(Method::GET)
        //     .uri("/actuator/health/liveness")
        //     .body(Body::empty())
        //     .unwrap();

        // let response = app.ready().await.unwrap().call(request).await.unwrap();
        // assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        // let request = Request::builder()
        //     .method(Method::GET)
        //     .uri("/actuator/health/readiness")
        //     .body(Body::empty())
        //     .unwrap();

        // let response = app.ready().await.unwrap().call(request).await.unwrap();
        // assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}


