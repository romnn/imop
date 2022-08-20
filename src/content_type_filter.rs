use http_headers::ContentType; //  , HeaderMap, HeaderMapExt, HeaderValue};
use mime_guess::mime;
use std::collections::HashSet;

pub trait ContentTypeFilter {
    fn should_compress(&self, content_type: Option<ContentType>) -> bool;
}

pub enum CompressContentType {
    All,
    Include(HashSet<mime::Mime>),
    Exclude(HashSet<mime::Mime>),
}

impl CompressContentType {
    pub fn include<I: IntoIterator<Item = mime::Mime>>(iter: I) -> Self {
        Self::Include(HashSet::from_iter(iter.into_iter()))
    }
    pub fn exclude<I: IntoIterator<Item = mime::Mime>>(iter: I) -> Self {
        Self::Exclude(HashSet::from_iter(iter.into_iter()))
    }
}

impl Default for CompressContentType {
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
                    "text/css",
                    "text/html",
                    "text/javascript",
                    "text/plain",
                    "text/xml",
                ]
                .iter()
                .map(|m| m.parse().expect("valid mime type"))
                .collect();
        }
        Self::include(DEFAULT_MIME_TO_COMPRESS.clone())
    }
}

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
    fn should_compress(&self, content_type: Option<ContentType>) -> bool {
        let mime: Option<mime::Mime> = content_type.map(Into::into);
        match self {
            CompressContentType::All => true,
            CompressContentType::Include(include) => mime
                .map(|mime| {
                    include
                        .iter()
                        .any(|included| mime_subset_of(&mime, included))
                })
                .unwrap_or(false),
            CompressContentType::Exclude(exclude) => mime
                .map(|mime| {
                    exclude
                        .iter()
                        .all(|excluded| !mime_subset_of(&mime, excluded))
                })
                .unwrap_or(true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CompressContentType, ContentTypeFilter};
    // use anyhow::Result;
    use http_headers::ContentType;
    use mime_guess::mime;

    fn content_type<S: AsRef<str>>(content_type: S) -> ContentType {
        ContentType::from(content_type.as_ref().parse::<mime::Mime>().unwrap())
    }

    #[test]
    fn test_compress_content_type_special_cases() {
        assert_eq!(
            CompressContentType::exclude(vec![]).should_compress(Some(content_type("image/png"))),
            true
        );
        assert_eq!(
            CompressContentType::include(vec![]).should_compress(Some(content_type("image/png"))),
            false
        );
        assert_eq!(
            CompressContentType::All.should_compress(Some(content_type("image/png"))),
            true
        );
        assert_eq!(
            CompressContentType::include(vec![]).should_compress(None),
            false
        );
        assert_eq!(
            CompressContentType::exclude(vec![]).should_compress(None),
            true
        );
    }

    #[test]
    fn test_compress_content_type_exlude_images() {
        let test = ContentType::from("image/png".parse::<mime::Mime>().unwrap());
        let f = CompressContentType::exclude(vec![mime::IMAGE_STAR]);
        assert_eq!(f.should_compress(Some(content_type("image/png"))), false);
        assert_eq!(f.should_compress(Some(content_type("image/jpeg"))), false);
        assert_eq!(f.should_compress(Some(content_type("image/*"))), false);
        assert_eq!(f.should_compress(Some(content_type("text/html"))), true);
        assert_eq!(f.should_compress(Some(content_type("text/*"))), true);
        assert_eq!(f.should_compress(Some(content_type("text/*"))), true);
        // CompressContentType::include(vec![]),
        // CompressContentType::exclude(vec![]),
        // CompressContentType::exclude(vec![mime::IMAGE_STAR]),
        // Ok(())
    }

    #[test]
    fn test_compress_content_type_default() {
        let f = CompressContentType::default();
        assert_eq!(f.should_compress(Some(content_type("image/png"))), false);
        assert_eq!(f.should_compress(Some(content_type("image/jpeg"))), false);
        assert_eq!(f.should_compress(Some(content_type("image/*"))), false);
        assert_eq!(f.should_compress(Some(content_type("text/html"))), true);
        assert_eq!(f.should_compress(Some(content_type("text/*"))), true);
        assert_eq!(f.should_compress(Some(content_type("text/*"))), true);
        assert_eq!(
            f.should_compress(Some(content_type("application/wasm"))),
            true
        );
        assert_eq!(
            f.should_compress(Some(content_type("application/json"))),
            true
        );
    }
}
