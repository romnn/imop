#![allow(warnings)]

pub mod cache;
#[cfg(feature = "compression")]
pub mod compression;
pub mod conditionals;
pub mod file;
pub mod headers;
pub mod image;

use warp::Filter;

pub trait FilterClone: Filter + Clone {}

impl<T> FilterClone for T where T: Filter + Clone {}
