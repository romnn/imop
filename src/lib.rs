// #![allow(warnings)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]

pub mod bounds;
#[cfg(feature = "cache")]
pub mod cache;
#[cfg(feature = "compression")]
pub mod compression;
pub mod conditionals;
#[cfg(feature = "compression")]
pub mod content_type_filter;
mod debug;
pub mod file;
pub mod headers;
pub mod image;
pub mod mime;

use warp::Filter;

pub trait FilterClone: Filter + Clone {}

impl<T> FilterClone for T where T: Filter + Clone {}
