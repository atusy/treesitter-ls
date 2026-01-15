//! Actor components for async bridge connection.
//!
//! This module provides the actor-based infrastructure for non-blocking
//! communication with downstream language servers (ADR-0015).
//!
//! # Components
//!
//! - `ResponseRouter`: Routes responses to pending requests via oneshot channels
//! - `Reader`: Background task that reads from server stdout and routes responses

mod reader;
mod response_router;

pub(crate) use reader::{ReaderTaskHandle, spawn_reader_task};
pub(crate) use response_router::ResponseRouter;
