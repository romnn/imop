use super::headers;
use std::convert::Infallible;
use warp::{http::StatusCode, hyper, reply, Filter};

#[derive(Default, Debug)]
pub struct Conditionals {
    if_modified_since: Option<headers::IfModifiedSince>,
    if_unmodified_since: Option<headers::IfUnmodifiedSince>,
    if_range: Option<headers::IfRange>,
    range: Option<headers::Range>,
}

pub enum Cond {
    NoBody(reply::Response),
    WithBody(Option<headers::Range>),
}

impl Conditionals {
    #[inline]
    pub fn check(self, last_modified: Option<headers::LastModified>) -> Cond {
        if let Some(since) = self.if_unmodified_since {
            let precondition =
                last_modified.map_or(false, |time| since.precondition_passes(time.into()));

            if !precondition {
                let mut res = reply::Response::new(hyper::Body::empty());
                *res.status_mut() = StatusCode::PRECONDITION_FAILED;
                return Cond::NoBody(res);
            }
        }

        if let Some(since) = self.if_modified_since {
            let unmodified = last_modified
                // no last_modified means its always modified
                .map_or(false, |time| !since.is_modified(time.into()));
            if unmodified {
                let mut res = reply::Response::new(hyper::Body::empty());
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

#[inline]
#[must_use]
pub fn conditionals() -> impl Filter<Extract = (Conditionals,), Error = Infallible> + Copy {
    use headers::HeaderMapExt;
    warp::header::headers_cloned().map(|headers: headers::HeaderMap| Conditionals {
        if_modified_since: headers.typed_get(),
        if_unmodified_since: headers.typed_get(),
        if_range: headers.typed_get(),
        range: headers.typed_get(),
    })
}
