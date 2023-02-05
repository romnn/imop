use http_headers::ContentType; //  , HeaderMap, HeaderMapExt, HeaderValue};
use mime_guess::mime;
use std::collections::HashSet;

pub trait ContentTypeFilter {
    fn should_compress(&self, content_type: Option<ContentType>) -> bool;
}

#[derive(Debug, Clone)]
pub enum CompressContentType {
    All,
    Include(HashSet<mime::Mime>),
    Exclude(HashSet<mime::Mime>),
}

impl CompressContentType {
    #[inline]
    pub fn include(iter: impl IntoIterator<Item = mime::Mime>) -> Self {
        Self::Include(iter.into_iter().collect())
    }

    #[inline]
    pub fn exclude(iter: impl IntoIterator<Item = mime::Mime>) -> Self {
        Self::Exclude(iter.into_iter().collect())
    }
}

impl Default for CompressContentType {
    #[inline]
    fn default() -> Self {
        lazy_static::lazy_static! {
            static ref DEFAULT_MIME_TO_COMPRESS: Vec<mime::Mime> = vec![
                    "application/atom+xml",
                    "application/geo+json",
                    "application/javascript",
                    "application/x-javascript",
                    "application/json",
                    "application/ld+json",
                    "application/manifest+json",
                    "application/rdf+xml",
                    "application/rss+xml",
                    "application/xhtml+xml",
                    "application/xml",
                    "application/wasm",
                    "font/eot",
                    "font/otf",
                    "font/ttf",
                    "image/svg+xml",
                    "text/*",
                    // "text/css",
                    // "text/html",
                    // "text/javascript",
                    // "text/plain",
                    // "text/xml",
                ]
                .iter()
                .map(|m| m.parse().expect("valid mime type"))
                .collect();
        }
        Self::include(DEFAULT_MIME_TO_COMPRESS.clone())
    }
}

#[inline]
#[must_use]
pub fn mime_subset_of(candidate: &mime::Mime, subset_of: &mime::Mime) -> bool {
    // println!("candidate: {} {}", candidate.type_(), candidate.subtype());
    // println!("subset_of: {} {}", subset_of.type_(), subset_of.subtype());

    if candidate.type_() != subset_of.type_() {
        return false;
    }
    if subset_of.subtype() == "*" {
        return true;
    }
    candidate.subtype() == subset_of.subtype()
}

impl ContentTypeFilter for CompressContentType {
    #[inline]
    fn should_compress(&self, content_type: Option<ContentType>) -> bool {
        let mime: Option<mime::Mime> = content_type.map(Into::into);
        // dbg!(&mime);
        // dbg!(&self);
        match self {
            CompressContentType::All => true,
            CompressContentType::Include(include) => mime.map_or(false, |mime| {
                include
                    .iter()
                    .any(|included| mime_subset_of(&mime, included))
            }),
            CompressContentType::Exclude(exclude) => mime.map_or(true, |mime| {
                exclude
                    .iter()
                    .all(|excluded| !mime_subset_of(&mime, excluded))
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CompressContentType, ContentTypeFilter};
    use http_headers::ContentType;
    use mime_guess::mime;

    fn content_type<S: AsRef<str>>(content_type: S) -> ContentType {
        ContentType::from(content_type.as_ref().parse::<mime::Mime>().unwrap())
    }

    #[test]
    fn test_compress_content_type_special_cases() {
        assert!(
            CompressContentType::exclude(vec![]).should_compress(Some(content_type("image/png")))
        );
        assert!(
            !CompressContentType::include(vec![]).should_compress(Some(content_type("image/png")))
        );
        assert!(CompressContentType::All.should_compress(Some(content_type("image/png"))));
        assert!(!CompressContentType::include(vec![]).should_compress(None));
        assert!(CompressContentType::exclude(vec![]).should_compress(None));
    }

    #[test]
    fn test_compress_content_type_exlude_images() {
        let f = CompressContentType::exclude(vec![mime::IMAGE_STAR]);
        assert!(!f.should_compress(Some(content_type("image/png"))));
        assert!(!f.should_compress(Some(content_type("image/jpeg"))));
        assert!(!f.should_compress(Some(content_type("image/*"))));
        assert!(f.should_compress(Some(content_type("text/html"))));
        assert!(f.should_compress(Some(content_type("text/*"))));
    }

    #[test]
    fn test_compress_content_type_default() {
        let f = CompressContentType::default();
        assert!(!f.should_compress(Some(content_type("image/png"))));
        assert!(!f.should_compress(Some(content_type("image/jpeg"))));
        assert!(!f.should_compress(Some(content_type("image/*"))));
        assert!(f.should_compress(Some(content_type("text/html"))));
        assert!(f.should_compress(Some(content_type("text/*"))));
        assert!(f.should_compress(Some(content_type("application/wasm"))));
        assert!(f.should_compress(Some(content_type("application/json"))));
    }
}
