use std::convert::TryFrom;

use super::content_coding::ContentCoding;
use super::quality_value::{QualityValue, TryFromValues};
use http_headers::{Header, HeaderName, HeaderValue};

/// `Accept-Encoding` header, defined in
/// [RFC7231](https://tools.ietf.org/html/rfc7231#section-5.3.4)
///
/// The `Accept-Encoding` header field can be used by user agents to
/// indicate what response content-codings are acceptable in the response.
/// An "identity" token is used as a synonym for "no encoding" in
/// order to communicate when no encoding is preferred.
///
/// # ABNF
///
/// ```text
/// Accept-Encoding  = #( codings [ weight ] )
/// codings          = content-coding / "identity" / "*"
/// ```
///
/// # Example Values
///
/// * `gzip`
/// * `br;q=1.0, gzip;q=0.8`
///

#[derive(Clone, Debug)]
pub struct AcceptEncoding(pub QualityValue);

impl Header for AcceptEncoding {
    fn name() -> &'static HeaderName {
        &http::header::ACCEPT_ENCODING
    }

    fn decode<'i, I>(values: &mut I) -> Result<Self, http_headers::Error>
    where
        I: Iterator<Item = &'i HeaderValue>,
    {
        Ok(AcceptEncoding(QualityValue::try_from_values(values)?))
    }

    fn encode<E>(&self, values: &mut E)
    where
        E: Extend<HeaderValue>,
    {
        let value = HeaderValue::from(&self.0);
        values.extend(std::iter::once(value));
    }
}

impl AcceptEncoding {
    /// Convience method to create an `Accept-Encoding: gzip` header
    #[inline]
    #[must_use]
    pub fn gzip() -> AcceptEncoding {
        AcceptEncoding(HeaderValue::from_static("gzip").into())
    }

    /// A convience method to create an Accept-Encoding header from pairs of values and qualities
    ///
    /// # Example
    ///
    /// ```
    /// use imop::headers::AcceptEncoding;
    ///
    /// let pairs = vec![("gzip", 1.0), ("deflate", 0.8)];
    /// let header = AcceptEncoding::from_quality_pairs(&mut pairs.into_iter());
    /// ```
    pub fn from_quality_pairs<'i, I>(pairs: &mut I) -> Result<AcceptEncoding, http_headers::Error>
    where
        I: Iterator<Item = (&'i str, f32)>,
    {
        let values: Vec<HeaderValue> = pairs
            .map(|pair| <QualityValue>::try_from(pair).map(HeaderValue::from))
            .collect::<Result<Vec<HeaderValue>, http_headers::Error>>()?;
        let value = QualityValue::try_from_values(&mut values.iter())?;
        Ok(AcceptEncoding(value))
    }

    /// Returns the most prefered encoding that is specified by the header,
    /// if one is specified.
    ///
    /// Note: This peeks at the underlying iter, not modifying it.
    ///
    /// # Example
    ///
    /// ```
    /// use imop::headers::{AcceptEncoding, ContentCoding};
    ///
    /// let pairs = vec![("gzip", 1.0), ("deflate", 0.8)];
    /// let accept_enc = AcceptEncoding::from_quality_pairs(&mut pairs.into_iter()).unwrap();
    /// let mut encodings = accept_enc.sorted_encodings();
    ///
    /// assert_eq!(accept_enc.prefered_encoding(), Some(ContentCoding::GZIP));
    /// ```
    pub fn prefered_encoding(&self) -> Option<ContentCoding> {
        self.0
            .iter()
            .peekable()
            .peek()
            .map(|s| ContentCoding::from_name(*s))
    }

    /// Returns a quality sorted iterator of the `ContentCoding`
    ///
    /// # Example
    ///
    /// ```
    /// use imop::headers::{AcceptEncoding, ContentCoding, HeaderValue};
    ///
    /// let val = HeaderValue::from_static("deflate, gzip;q=1.0, br;q=0.8");
    /// let accept_enc = AcceptEncoding(val.into());
    /// let mut encodings = accept_enc.sorted_encodings();
    ///
    /// assert_eq!(encodings.next(), Some(ContentCoding::DEFLATE));
    /// assert_eq!(encodings.next(), Some(ContentCoding::GZIP));
    /// assert_eq!(encodings.next(), Some(ContentCoding::BROTLI));
    /// assert_eq!(encodings.next(), None);
    /// ```
    pub fn sorted_encodings(&self) -> impl Iterator<Item = ContentCoding> + '_ {
        self.0.iter().map(ContentCoding::from_name)
    }

    /// Returns a quality sorted iterator of values
    ///
    /// # Example
    ///
    /// ```
    /// use imop::headers::{HeaderValue, AcceptEncoding, ContentCoding};
    ///
    /// let val = HeaderValue::from_static("deflate, gzip;q=1.0, br;q=0.8");
    /// let accept_enc = AcceptEncoding(val.into());
    /// let mut encodings = accept_enc.sorted_values();
    ///
    /// assert_eq!(encodings.next(), Some("deflate"));
    /// assert_eq!(encodings.next(), Some("gzip"));
    /// assert_eq!(encodings.next(), Some("br"));
    /// assert_eq!(encodings.next(), None);
    /// ```
    pub fn sorted_values(&self) -> impl Iterator<Item = &str> {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::AcceptEncoding;
    use crate::headers::{ContentCoding, HeaderValue};

    #[test]
    fn from_static() {
        let val = HeaderValue::from_static("deflate, gzip;q=1.0, br;q=0.9");
        let accept_enc = AcceptEncoding(val.into());

        assert_eq!(accept_enc.prefered_encoding(), Some(ContentCoding::DEFLATE));

        let mut encodings = accept_enc.sorted_encodings();
        assert_eq!(encodings.next(), Some(ContentCoding::DEFLATE));
        assert_eq!(encodings.next(), Some(ContentCoding::GZIP));
        assert_eq!(encodings.next(), Some(ContentCoding::BROTLI));
        assert_eq!(encodings.next(), None);
    }

    #[test]
    fn from_pairs() {
        let pairs = vec![("gzip", 1.0), ("br", 0.9)];
        let accept_enc = AcceptEncoding::from_quality_pairs(&mut pairs.into_iter()).unwrap();

        assert_eq!(accept_enc.prefered_encoding(), Some(ContentCoding::GZIP));

        let mut encodings = accept_enc.sorted_encodings();
        assert_eq!(encodings.next(), Some(ContentCoding::GZIP));
        assert_eq!(encodings.next(), Some(ContentCoding::BROTLI));
        assert_eq!(encodings.next(), None);
    }
}
