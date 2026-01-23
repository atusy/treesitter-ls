//! Actor components for async bridge connection.
//!
//! This module provides the actor-based infrastructure for non-blocking
//! communication with downstream language servers (ADR-0015).
//!
//! # Components
//!
//! - `ResponseRouter`: Routes responses to pending requests via oneshot channels
//! - `Reader`: Background task that reads from server stdout and routes responses
//! - `DownstreamMessageHandler`: Forwards notifications from downstream servers to client

mod downstream_handler;
mod downstream_message;
mod reader;
mod response_router;

pub(crate) use downstream_handler::{DownstreamHandlerHandle, spawn_downstream_handler};
pub(crate) use downstream_message::{DownstreamMessage, DownstreamNotification};
// VirtualDocContext is defined but not yet used - will be used in Phase 1
#[allow(unused_imports)]
pub(crate) use downstream_message::VirtualDocContext;
#[cfg(test)]
pub(crate) use reader::spawn_reader_task;
#[cfg(test)]
pub(crate) use reader::spawn_reader_task_with_liveness;
pub(crate) use reader::{ReaderTaskHandle, spawn_reader_task_for_language};
pub(crate) use response_router::ResponseRouter;
