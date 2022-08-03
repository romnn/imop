use http_headers::HeaderValue;

// #[derive(thiserror::Error, Debug, PartialEq)]
// pub enum Error {
//     #[error("invalid content coding: `{0}`")]
//     Invalid(String),
// }

// Derives an enum to represent content codings and some helpful impls
macro_rules! define_content_coding {
    ($($coding:ident; $str:expr,)+) => {
        #[derive(Copy, Clone, Debug, Eq, PartialEq)]
        /// Values that are used with headers like [`Content-Encoding`](self::ContentEncoding) or
        /// [`Accept-Encoding`](self::AcceptEncoding)
        ///
        /// [RFC7231](https://www.iana.org/assignments/http-parameters/http-parameters.xhtml)
        pub enum ContentCoding {
            $(
                #[doc = $str]
                $coding,
            )+
        }

        impl ContentCoding {
            /// Returns a `&'static str` for a `ContentCoding`
            ///
            /// # Example
            ///
            /// ```
            /// use imop::headers::ContentCoding;
            ///
            /// let coding = ContentCoding::BROTLI;
            /// assert_eq!(coding.to_static(), "br");
            /// ```
            #[inline]
            pub fn to_static(&self) -> &'static str {
                match *self {
                    $(ContentCoding::$coding => $str,)+
                }
            }

            /// Given a `&str` returns a `ContentCoding`
            ///
            /// Note this will never fail, in the case of `&str` being an invalid content coding,
            /// will return `ContentCoding::IDENTITY` because `'identity'` is generally always an
            /// accepted coding.
            ///
            /// # Example
            ///
            /// ```
            /// use imop::headers::ContentCoding;
            ///
            /// let invalid = ContentCoding::from_name("not a valid coding");
            /// assert_eq!(invalid, ContentCoding::IDENTITY);
            ///
            /// let valid = ContentCoding::from_name("gzip");
            /// assert_eq!(valid, ContentCoding::GZIP);
            /// ```
            #[inline]
            pub fn from_name(s: &str) -> Self {
                ContentCoding::try_from_name(s).unwrap_or(ContentCoding::IDENTITY)
            }

            /// Given a `&str` will try to return a `ContentCoding`
            ///
            /// Different from `ContentCoding::from_name(&str)`, if `&str` is an invalid content
            /// coding, it will return `Err(())`
            ///
            /// # Example
            ///
            /// ```
            /// use imop::headers::ContentCoding;
            ///
            /// let invalid = ContentCoding::try_from_name("not a valid coding");
            /// assert!(invalid.is_err());
            ///
            /// let valid = ContentCoding::try_from_name("gzip");
            /// assert_eq!(valid.unwrap(), ContentCoding::GZIP);
            /// ```
            #[inline]
            pub fn try_from_name(s: &str) -> Result<Self, http_headers::Error> {
                match s {
                    $(
                        stringify!($coding)
                        | $str => Ok(ContentCoding::$coding),
                    )+
                    _ => Err(http_headers::Error::invalid())
                    // _ => Err(Error::Invalid(s.to_owned()))
                }
            }
        }

        impl std::string::ToString for ContentCoding {
            #[inline]
            fn to_string(&self) -> String {
                match *self {
                    $(ContentCoding::$coding => $str.to_string(),)+
                }
            }
        }

        impl From<ContentCoding> for HeaderValue {
            fn from(coding: ContentCoding) -> HeaderValue {
                match coding {
                    $(ContentCoding::$coding => HeaderValue::from_static($str),)+
                }
            }
        }
    }
}

define_content_coding! {
    BROTLI; "br",
    COMPRESS; "compress",
    DEFLATE; "deflate",
    GZIP; "gzip",
    // todo: allow more?
    IDENTITY; "identity",
}

#[cfg(test)]
mod tests {
    use super::ContentCoding;
    use crate::headers::Error;

    #[test]
    fn to_static() {
        assert_eq!(ContentCoding::GZIP.to_static(), "gzip");
    }

    #[test]
    fn to_string() {
        assert_eq!(ContentCoding::DEFLATE.to_string(), "deflate".to_string());
    }

    #[test]
    fn from_name() {
        assert_eq!(ContentCoding::from_name("br"), ContentCoding::BROTLI);
        assert_eq!(ContentCoding::from_name("GZIP"), ContentCoding::GZIP);
        assert_eq!(
            ContentCoding::from_name("blah blah"),
            ContentCoding::IDENTITY
        );
    }

    #[test]
    fn try_from_name() {
        assert_eq!(
            ContentCoding::try_from_name("br").unwrap(),
            ContentCoding::BROTLI
        );
        assert!(ContentCoding::try_from_name("blah blah").is_err());
        // assert_eq!(
        //     ContentCoding::try_from_name(&invalid),
        //     // Err(Error::Invalid(invalid.to_owned()))
        //     Err(http_headers::Error::invalid())
        // );
    }
}
