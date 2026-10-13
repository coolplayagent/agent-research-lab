//! Compatibility re-exports of the single Native effects context.
pub use native_effects::{
    CaptureObserver, CaptureRequest, CaptureReservation, CapturedBytes, CapturedControl,
    ObserverContext, ObserverGuard, cleanup_scope, observer_scope,
};
