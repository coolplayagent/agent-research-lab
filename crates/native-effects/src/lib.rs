//! One optional observation context for Native control and parent filesystem effects.
mod capture;
mod context;
pub mod fs;
mod policy;
mod reservation;
pub use capture::*;
pub use context::{
    FilesystemObserver, FilesystemReservation, NativeObservationSession, ObservationFence,
    ObserverContext, ObserverGuard, attempt_scope, capture_scope, cleanup_scope, control_scope,
    observation_scope, observer_scope,
};
pub use policy::{FilesystemPolicy, FilesystemRoot};

#[cfg(test)]
mod tests;
