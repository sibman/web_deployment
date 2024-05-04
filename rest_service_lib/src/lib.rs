//! Provides a RESTful web server managing some Todos.
//!
//! API will be:
//!
//! - `GET /todos`: return a JSON list of Todos.
//! - `POST /todos`: create a new Todo.
//! - `PUT or PATCH /todos/:id`: update a specific Todo.
//! - `DELETE /todos/:id`: delete a specific Todo.
//!
//! Run with
//!
//! ```not_rust
//! cargo run -p rest_service
//! ```

pub mod api {
    use axum::{
        error_handling::HandleErrorLayer,
        extract::{Path, Query, State},
        http::StatusCode,
        response::IntoResponse,
        routing::{get, post, put},
        Json, Router,
    };
    use serde::{Deserialize, Serialize};
    use std::time::Duration;
    use std::{
        collections::HashMap,
        sync::{Arc, RwLock},
    };
    use tower::{BoxError, ServiceBuilder};
    use tower_http::trace::TraceLayer;

    use axum::extract::ConnectInfo;
    use axum::Extension;
    use rest_actuator::api::{ActuatorRouterBuilder, ActuatorState, StateChecker};
    use std::net::SocketAddr;
    use std::sync::Mutex;
    use utoipa::OpenApi;
    use utoipa::ToSchema;
    use utoipa_swagger_ui::SwaggerUi;
    use uuid::Uuid;

    #[derive(OpenApi)]
    #[openapi(
        paths(todos_index, todos_create, todos_update, todos_delete),
        components(schemas(Pagination, Todo, CreateTodo, UpdateTodo))
    )]
    struct ApiDoc;

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

    pub fn app() -> Router {
        let db = Db::default();

        let mut actuator_state = ActuatorState::new();

        // Add health checkers
        actuator_state.add_health_checker(
            "database".to_string(),
            Arc::new(Mutex::new(Box::new(DatabaseHealthCheck {
                ready: true,
                alive: true,
            }))),
        );

        let extension: Option<Extension<ActuatorState>> = Some(Extension(actuator_state));

        let router = ActuatorRouterBuilder::new(Router::new())
            .with_readiness_route()
            .with_liveness_route()
            .with_info_route()
            .with_health_route()
            .with_layer(extension)
            .build();

        // Compose the routes
        router
            .route("/todos", get(todos_index).post(todos_create))
            .route(
                "/todos/:id",
                put(todos_update).patch(todos_update).delete(todos_delete),
            )
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
            .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
            // Add middleware to all routes
            .layer(
                ServiceBuilder::new()
                    .layer(HandleErrorLayer::new(|error: BoxError| async move {
                        if error.is::<tower::timeout::error::Elapsed>() {
                            Ok(StatusCode::REQUEST_TIMEOUT)
                        } else {
                            Err((
                                StatusCode::INTERNAL_SERVER_ERROR,
                                format!("Unhandled internal error: {error}"),
                            ))
                        }
                    }))
                    .timeout(Duration::from_secs(10))
                    .layer(TraceLayer::new_for_http())
                    .into_inner(),
            )
            .with_state(db)
    }

    // The query parameters for todos index
    #[derive(Debug, Deserialize, Default, ToSchema)]
    struct Pagination {
        pub offset: Option<usize>,
        pub limit: Option<usize>,
    }

    /// Get todos
    ///
    /// Get todos from database
    #[utoipa::path(
    get,
    path = "/todos",
    responses(
        (status = 200, description = "Todos found successfully", body = [Todo])
    ),
    params(
        ("pagination" = Option<Pagination>, Query, description = "Todo database pagination to retrieve by offset and limit"),
    )
    )]
    async fn todos_index(
        pagination: Option<Query<Pagination>>,
        State(db): State<Db>,
    ) -> impl IntoResponse {
        let todos = db.read().unwrap();

        let Query(pagination) = pagination.unwrap_or_default();

        let todos = todos
            .values()
            .skip(pagination.offset.unwrap_or(0))
            .take(pagination.limit.unwrap_or(usize::MAX))
            .cloned()
            .collect::<Vec<_>>();

        Json(todos)
    }

    #[derive(Debug, Deserialize, ToSchema)]
    struct CreateTodo {
        text: String,
    }

    /// Create todo
    ///
    /// Create todo in database with auto generate uuid v4
    #[utoipa::path(
    post,
    path = "/todos",
    responses(
        (status = 201, description = "Create todo successfully", body = Todo)
    )
    )]
    async fn todos_create(
        State(db): State<Db>,
        Json(input): Json<CreateTodo>,
    ) -> impl IntoResponse {
        let todo = Todo {
            id: Uuid::new_v4(),
            text: input.text,
            completed: false,
        };

        db.write().unwrap().insert(todo.id, todo.clone());

        (StatusCode::CREATED, Json(todo))
    }

    #[derive(Debug, Deserialize, ToSchema)]
    struct UpdateTodo {
        text: Option<String>,
        completed: Option<bool>,
    }

    /// Update todo by id
    ///
    /// Update todo in database by todo id
    #[utoipa::path(
    put,
    path = "/todos/{id}",
    responses(
        (status = 200, description = "Todo updated successfully", body = Todo),
        (status = NOT_FOUND, description = "Todo was not found")
    ),
    params(
        ("id" = Path<Uuid>, Path, description = "Todo database id to update Todo for"),
    )
    )]
    async fn todos_update(
        Path(id): Path<Uuid>,
        State(db): State<Db>,
        Json(input): Json<UpdateTodo>,
    ) -> Result<impl IntoResponse, StatusCode> {
        let mut todo = db
            .read()
            .unwrap()
            .get(&id)
            .cloned()
            .ok_or(StatusCode::NOT_FOUND)?;

        if let Some(text) = input.text {
            todo.text = text;
        }

        if let Some(completed) = input.completed {
            todo.completed = completed;
        }

        db.write().unwrap().insert(todo.id, todo.clone());

        Ok(Json(todo))
    }

    /// Delete todo by id
    ///
    /// Delete todo from database by todo id
    #[utoipa::path(
    delete,
    path = "/todos/{id}",
    responses(
        (status = NO_CONTENT, description = "Todo deleted successfully"),
        (status = NOT_FOUND, description = "Todo was not found")
    ),
    params(
        ("id" = Path<Uuid>, Path, description = "Todo database id to delete Todo for"),
    )
    )]
    async fn todos_delete(Path(id): Path<Uuid>, State(db): State<Db>) -> impl IntoResponse {
        if db.write().unwrap().remove(&id).is_some() {
            StatusCode::NO_CONTENT
        } else {
            StatusCode::NOT_FOUND
        }
    }

    type Db = Arc<RwLock<HashMap<Uuid, Todo>>>;

    #[derive(Debug, Serialize, Clone, ToSchema)]
    struct Todo {
        id: Uuid,
        text: String,
        completed: bool,
    }
}

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

    #[tokio::test]
    async fn todos_get() {
        let app = api::app();

        // `Router` implements `tower::Service<Request<Body>>` so we can
        // call it like any tower service, no need to run an HTTP server.
        let response = app
            .oneshot(
                Request::builder()
                    .method(http::Method::GET)
                    .uri("/todos")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"[]");
    }

    #[tokio::test]
    async fn todos_get_plus_query() {
        let app = api::app();

        // `Router` implements `tower::Service<Request<Body>>` so we can
        // call it like any tower service, no need to run an HTTP server.
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(http::Method::GET)
                    .uri("/todos?offset=0&limit=0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"[]");

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(http::Method::GET)
                    .uri("/todos?offset=0&limit=2")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"[]");
    }

    #[tokio::test]
    async fn json() {
        let app = api::app();

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

    #[tokio::test]
    async fn not_found() {
        let app = api::app();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/does-not-exist")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(body.is_empty());
    }

    // You can also spawn a server and talk to it like any other HTTP server:
    #[tokio::test]
    async fn the_real_deal() {
        let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, api::app()).await.unwrap();
        });

        let client =
            hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
                .build_http();

        let response = client
            .request(
                Request::builder()
                    .uri(format!("http://{addr}/todos"))
                    .header("Host", "localhost")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"[]");
    }

    // You can use `ready()` and `call()` to avoid using `clone()`
    // in multiple request
    #[tokio::test]
    async fn multiple_request() {
        let mut app = api::app().into_service();

        let request = Request::builder()
            .method(http::Method::GET)
            .uri("/todos")
            .body(Body::empty())
            .unwrap();
        let response = ServiceExt::<Request<Body>>::ready(&mut app)
            .await
            .unwrap()
            .call(request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = Request::builder()
            .method(http::Method::GET)
            .uri("/todos")
            .body(Body::empty())
            .unwrap();
        let response = ServiceExt::<Request<Body>>::ready(&mut app)
            .await
            .unwrap()
            .call(request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    // Here we're calling `/requires-connect-info` which requires `ConnectInfo`
    //
    // That is normally set with `Router::into_make_service_with_connect_info` but we can't easily
    // use that during tests. The solution is instead to set the `MockConnectInfo` layer during
    // tests.
    #[tokio::test]
    async fn with_into_make_service_with_connect_info() {
        let mut app = api::app()
            .layer(MockConnectInfo(SocketAddr::from(([0, 0, 0, 0], 3000))))
            .into_service();

        let request = Request::builder()
            .uri("/requires-connect-info")
            .body(Body::empty())
            .unwrap();
        let response = app.ready().await.unwrap().call(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
