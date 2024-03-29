use std::{num::ParseIntError, string::FromUtf8Error};

use axum::{http::StatusCode, response::IntoResponse};
use thiserror::Error;
use tracing::error;

#[derive(Debug, Error)]
pub enum KnawledgeError {
    #[error("IO: {0}")]
    IO(#[from] std::io::Error),

    #[error("UTF-8: {0}")]
    Utf8(#[from] FromUtf8Error),

    #[error("Parse int: {0}")]
    Parse(#[from] ParseIntError),

    #[error("Template: {0}")]
    MiniJinja(#[from] minijinja::Error),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Inotify error: {0}")]
    Watcher(#[from] notify::Error),

    #[error("SQL: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("Does not exist: {0}")]
    DoesNotExist(String),

    #[error("Invalid Directory: {0}")]
    InvalidDirectory(String),

    #[error("YAML error: {0}")]
    SerdeYaml(#[from] serde_yaml::Error),
}

impl IntoResponse for KnawledgeError {
    fn into_response(self) -> axum::response::Response {
        error!("Error: {self}");
        match self {
            KnawledgeError::MiniJinja(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
            }
            KnawledgeError::IO(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
            }
            KnawledgeError::Parse(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
            }
            KnawledgeError::Utf8(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
            KnawledgeError::NotFound(e) => (StatusCode::NOT_FOUND, e).into_response(),
            KnawledgeError::Watcher(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
            }
            KnawledgeError::DoesNotExist(e) => (StatusCode::NOT_FOUND, e).into_response(),
            e => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    }
}
