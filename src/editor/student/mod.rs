//! Student Mode: lessons, tutor state, scoring and progress, kept apart from Guided Mode
//! and the agents runtime so it can later run as its own app.
pub(super) mod chart;
pub(super) mod curriculum;
pub(super) mod host;
pub(super) mod pedagogy;
pub(super) mod practice;
pub(super) mod progress;
pub(super) mod rubric;
pub(super) mod runner;
pub(super) mod sync;
pub(super) mod voice;
