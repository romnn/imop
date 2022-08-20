#![allow(warnings)]

pub mod cache;
#[cfg(feature = "compression")]
pub mod compression;
pub mod conditionals;
#[cfg(feature = "compression")]
mod content_type_filter;
pub mod file;
pub mod headers;
pub mod image;

use warp::Filter;

pub trait FilterClone: Filter + Clone {}

impl<T: Filter + Clone> FilterClone for T {}
