//! Native navigation to Project threads identified from local files.
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub(crate) use macos::{open_project, PROJECTS_SUPPORTED};
#[cfg(not(target_os = "macos"))]
mod unsupported;
#[cfg(not(target_os = "macos"))]
pub(crate) use unsupported::{open_project, PROJECTS_SUPPORTED};
