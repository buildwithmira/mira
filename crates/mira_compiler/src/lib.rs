//! The Mira compiler: turns a project's routes, layouts, and content
//! collections into static HTML with native page transitions.

pub mod build;
pub mod config;
pub mod content;
pub mod migrate;
pub mod scaffold;
pub mod schema;
pub mod template;

mod assets;
mod hosts;
mod html;
mod lint;
mod media;
mod outputs;
mod pixel;
mod search;
mod twin;

pub use assets::{DEV_JS, OVERLAY_CSS};
pub use build::{BuildOptions, BuildReport, PageReport, Timing, build};
pub use config::Config;
