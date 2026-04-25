use crate::analysis::frame::TrackFrame;

/// Placeholder registry for Phase 1/2.
/// Phase 3 will replace this with shared memory backed by the `shared_memory` crate.
pub struct Registry;

impl Registry {
    pub fn new() -> Self {
        Self
    }

    /// Returns all active track frames visible to the master instance.
    pub fn read_tracks(&self) -> Vec<(String, TrackFrame)> {
        Vec::new()
    }

    pub fn write_frame(&self, _track_name: &str, _frame: &TrackFrame) {
        // No-op until Phase 3
    }
}
