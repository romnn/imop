#![allow(warnings)]

use super::headers::{
    AcceptRanges, ContentEncoding, ContentLength, ContentRange, ContentType, Header, HeaderMap,
    HeaderMapExt, HeaderValue, IfModifiedSince, IfRange, IfUnmodifiedSince, LastModified, Range,
};
use anyhow::Result;
use bytes::{Bytes, BytesMut};
use clap::Parser;
use futures_util::future::Either;
use futures_util::TryFuture;
use futures_util::{future, ready, stream, FutureExt, Stream, StreamExt, TryFutureExt};
use pin_project::pin_project;
use serde::{Deserialize, Serialize};
use std::cmp;
use std::collections::HashMap;
use std::convert::Infallible;
use std::fmt;
use std::io;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::AsyncSeekExt;
use tokio::io::AsyncWriteExt;
use tokio::signal;
use tokio::sync::{broadcast, Mutex, RwLock};
use tokio_util::io::poll_read_buf;
use tokio_util::io::{ReaderStream, StreamReader};
use urlencoding::decode;
use warp::http::{StatusCode, Uri};
use warp::hyper;
use warp::reply::Response;
use warp::Future;
use warp::Rejection;
use warp::{Filter, Reply};

#[derive(Debug)]
pub struct Conditionals {
    if_modified_since: Option<IfModifiedSince>,
    if_unmodified_since: Option<IfUnmodifiedSince>,
    if_range: Option<IfRange>,
    range: Option<Range>,
}

pub enum Cond {
    NoBody(Response),
    WithBody(Option<Range>),
}

impl Conditionals {
    pub fn check(self, last_modified: Option<LastModified>) -> Cond {
        if let Some(since) = self.if_unmodified_since {
            let precondition = last_modified
                .map(|time| since.precondition_passes(time.into()))
                .unwrap_or(false);

            if !precondition {
                let mut res = Response::new(hyper::Body::empty());
                *res.status_mut() = StatusCode::PRECONDITION_FAILED;
                return Cond::NoBody(res);
            }
        }

        if let Some(since) = self.if_modified_since {
            let unmodified = last_modified
                .map(|time| !since.is_modified(time.into()))
                // no last_modified means its always modified
                .unwrap_or(false);
            if unmodified {
                let mut res = Response::new(hyper::Body::empty());
                *res.status_mut() = StatusCode::NOT_MODIFIED;
                return Cond::NoBody(res);
            }
        }

        if let Some(if_range) = self.if_range {
            let can_range = !if_range.is_modified(None, last_modified.as_ref());

            if !can_range {
                return Cond::WithBody(None);
            }
        }

        Cond::WithBody(self.range)
    }
}

pub fn conditionals() -> impl Filter<Extract = (Conditionals,), Error = Infallible> + Copy {
    warp::header::headers_cloned().map(|headers: HeaderMap| Conditionals {
        if_modified_since: headers.typed_get(),
        if_unmodified_since: headers.typed_get(),
        if_range: headers.typed_get(),
        range: headers.typed_get(),
    })
}
