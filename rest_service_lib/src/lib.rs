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

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
use utoipa::ToSchema;
use uuid::Uuid;

pub mod api {
// The query parameters for todos index
#[derive(Debug, Deserialize, Default, ToSchema)]
pub struct Pagination {
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
        (status = 200, description = "Todos found succesfully", body = [Todo])
    ),
    params(
        ("pagination" = Pagination, Query, description = "Todo database pagination to retrieve by ofset and limit"),
    )
)]
pub async fn todos_index(
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
pub struct CreateTodo {
    text: String,
}

/// Create todo
///
/// Cteate todo in database with auto genearate uuid v4
#[utoipa::path(
    post,
    path = "/todos",
    responses(
        (status = 201, description = "Create todo succesfully", body = Todo)
    )
)]
pub async fn todos_create(
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
pub struct UpdateTodo {
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
        (status = 200, description = "Todo updated succesfully", body = Todo),
        (status = NOT_FOUND, description = "Todod was not found")
    ),
    params(
        ("id" = Path<Uuid>, Path, description = "Todo database id to update Todo for"),
    )
)]
pub async fn todos_update(
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
        (status = NO_CONTENT, description = "Todo deleted succesfully"),
        (status = NOT_FOUND, description = "Todo was not found")
    ),
    params(
        ("id" = Path<Uuid>, Path, description = "Todo database id to delete Todo for"),
    )
)]
pub async fn todos_delete(Path(id): Path<Uuid>, State(db): State<Db>) -> impl IntoResponse {
    if db.write().unwrap().remove(&id).is_some() {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

pub type Db = Arc<RwLock<HashMap<Uuid, Todo>>>;

#[derive(Debug, Serialize, Clone, ToSchema)]
pub struct Todo {
    id: Uuid,
    text: String,
    completed: bool,
}
}
// Original code
pub fn add(left: usize, right: usize) -> usize {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
