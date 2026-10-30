//! ProjectFCG: a Gantt-chart project scheduler modelled on Microsoft Project.
//!
//! The library holds the scheduling engine and the document model; the
//! `app` module is the egui desktop front end.

pub mod analysis;
pub mod app;
pub mod board;
pub mod calendar;
pub mod duration;
pub mod format;
pub mod history;
pub mod io;
pub mod jira;
pub mod leveling;
pub mod model;
pub mod mspdi;
pub mod ops;
pub mod recurring;
pub mod resources;
pub mod schedule;
pub mod tracking;

pub use model::{Project, Task};
pub use schedule::schedule;
