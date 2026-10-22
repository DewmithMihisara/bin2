//! bin2 core: seal any file into a self-contained `.bin2` page, and get it back out.

pub mod detect;
#[allow(dead_code)]
mod html;
#[allow(dead_code)]
mod time;

pub use detect::{detect, Detected, Kind};
