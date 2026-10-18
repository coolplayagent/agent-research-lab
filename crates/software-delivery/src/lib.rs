//! Host-owned, bounded activation of an exact merged software snapshot.
mod admission;
mod build;
mod cli;
mod host;
mod journal;
mod model;
mod platform;
mod reconcile;
mod request;
mod service;
mod worker;

pub use reconcile::reconcile;
pub use request::request;
pub use worker::worker;

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
