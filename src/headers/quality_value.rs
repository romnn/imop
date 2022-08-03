use self::sealed::FlatCsv;
use self::sealed::SemiQ;
use http_headers::HeaderValue;
use std::marker::PhantomData;

/// A CSV list that respects the Quality Values syntax defined in
/// [RFC7321](https://tools.ietf.org/html/rfc7231#section-5.3.1)
///
/// Many of the request header fields for proactive negotiation use a
/// common parameter, named "q" (case-insensitive), to assign a relative
/// "weight" to the preference for that associated kind of content.  This
/// weight is referred to as a "quality value" (or "qvalue") because the
/// same parameter name is often used within server configurations to
/// assign a weight to the relative quality of the various
/// representations that can be selected for a resource.
///
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct QualityValue<QualSep = SemiQ> {
    csv: FlatCsv,
    _marker: PhantomData<QualSep>,
}

pub trait TryFromValues: Sized {
    fn try_from_values<'i, I>(values: &mut I) -> Result<Self, http_headers::Error>
    where
        Self: Sized,
        I: Iterator<Item = &'i HeaderValue>;
}

impl TryFromValues for HeaderValue {
    fn try_from_values<'i, I>(values: &mut I) -> Result<Self, http_headers::Error>
    where
        I: Iterator<Item = &'i HeaderValue>,
    {
        values
            .next()
            .cloned()
            .ok_or_else(http_headers::Error::invalid)
    }
}

mod sealed {
    use super::{QualityValue, TryFromValues};
    use bytes::BytesMut;
    use std::cmp::Ordering;
    use std::convert::{From, TryFrom};
    use std::fmt;
    use std::marker::PhantomData;

    use http_headers::HeaderValue;
    use itertools::Itertools;

    #[derive(Clone, PartialEq, Eq, Hash)]
    pub(super) struct FlatCsv<Sep = Comma> {
        pub value: HeaderValue,
        _marker: PhantomData<Sep>,
    }

    pub(super) trait Separator {
        const BYTE: u8;
        const CHAR: char;
    }

    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub(super) enum Comma {}

    impl Separator for Comma {
        const BYTE: u8 = b',';
        const CHAR: char = ',';
    }

    impl<Sep: Separator> FlatCsv<Sep> {
        pub(super) fn iter(&self) -> impl Iterator<Item = &str> {
            self.value.to_str().ok().into_iter().flat_map(|value_str| {
                let mut in_quotes = false;
                value_str
                    .split(move |c| {
                        if in_quotes {
                            if c == '"' {
                                in_quotes = false;
                            }
                            false // dont split
                        } else if c == Sep::CHAR {
                            true // split
                        } else {
                            if c == '"' {
                                in_quotes = true;
                            }
                            false // dont split
                        }
                        // else {
                        //     if c == Sep::CHAR {
                        //         true // split
                        //     } else {
                        //         if c == '"' {
                        //             in_quotes = true;
                        //         }
                        //         false // dont split
                        //     }
                        // }
                    })
                    .map(|item| item.trim())
            })
        }
    }

    impl<Sep: Separator> TryFromValues for FlatCsv<Sep> {
        fn try_from_values<'i, I>(values: &mut I) -> Result<Self, http_headers::Error>
        where
            I: Iterator<Item = &'i HeaderValue>,
        {
            let flat = values.collect();
            Ok(flat)
        }
    }

    impl<Sep> From<HeaderValue> for FlatCsv<Sep> {
        fn from(value: HeaderValue) -> Self {
            FlatCsv {
                value,
                _marker: PhantomData,
            }
        }
    }

    impl<'a, Sep> From<&'a FlatCsv<Sep>> for HeaderValue {
        fn from(flat: &'a FlatCsv<Sep>) -> HeaderValue {
            flat.value.clone()
        }
    }

    impl<Sep> fmt::Debug for FlatCsv<Sep> {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            fmt::Debug::fmt(&self.value, f)
        }
    }

    impl<'a, Sep: Separator> FromIterator<&'a HeaderValue> for FlatCsv<Sep> {
        fn from_iter<I>(iter: I) -> Self
        where
            I: IntoIterator<Item = &'a HeaderValue>,
        {
            let mut values = iter.into_iter();

            // Common case is there is only 1 value, optimize for that
            if let (1, Some(1)) = values.size_hint() {
                return values
                    .next()
                    .expect("size_hint claimed 1 item")
                    .clone()
                    .into();
            }

            // Otherwise, there are multiple, so this should merge them into 1.
            let mut buf = values
                .next()
                .cloned()
                .map(|val| BytesMut::from(val.as_bytes()))
                .unwrap_or_else(BytesMut::new);

            for val in values {
                buf.extend_from_slice(&[Sep::BYTE, b' ']);
                buf.extend_from_slice(val.as_bytes());
            }

            let val = HeaderValue::from_maybe_shared(buf.freeze())
                .expect("comma separated HeaderValues are valid");

            val.into()
        }
    }

    // TODO: would be great if there was a way to de-dupe these with above
    impl<Sep: Separator> FromIterator<HeaderValue> for FlatCsv<Sep> {
        fn from_iter<I>(iter: I) -> Self
        where
            I: IntoIterator<Item = HeaderValue>,
        {
            let mut values = iter.into_iter();

            // Common case is there is only 1 value, optimize for that
            if let (1, Some(1)) = values.size_hint() {
                return values.next().expect("size_hint claimed 1 item").into();
            }

            // Otherwise, there are multiple, so this should merge them into 1.
            let mut buf = values
                .next()
                .map(|val| BytesMut::from(val.as_bytes()))
                .unwrap_or_else(BytesMut::new);

            for val in values {
                buf.extend_from_slice(&[Sep::BYTE, b' ']);
                buf.extend_from_slice(val.as_bytes());
            }

            let val = HeaderValue::from_maybe_shared(buf.freeze())
                .expect("comma separated HeaderValues are valid");

            val.into()
        }
    }

    pub trait QualityDelimiter {
        const STR: &'static str;
    }

    /// enum that represents the ';q=' delimiter
    #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum SemiQ {}

    impl QualityDelimiter for SemiQ {
        const STR: &'static str = ";q=";
    }

    /// enum that represents the ';level=' delimiter (extremely rare)
    #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum SemiLevel {}

    impl QualityDelimiter for SemiLevel {
        const STR: &'static str = ";level=";
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct QualityMeta<'a, Sep = SemiQ> {
        pub data: &'a str,
        pub quality: u16,
        _marker: PhantomData<Sep>,
    }

    impl<Delm: QualityDelimiter + Ord> Ord for QualityMeta<'_, Delm> {
        fn cmp(&self, other: &Self) -> Ordering {
            other.quality.cmp(&self.quality)
        }
    }

    impl<Delm: QualityDelimiter + Ord> PartialOrd for QualityMeta<'_, Delm> {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            Some(self.cmp(other))
        }
    }

    impl<'a, Delm: QualityDelimiter> TryFrom<&'a str> for QualityMeta<'a, Delm> {
        type Error = http_headers::Error;

        fn try_from(val: &'a str) -> Result<Self, Self::Error> {
            let mut parts: Vec<&str> = val.split(Delm::STR).collect();

            match (parts.pop(), parts.pop()) {
                (Some(qual), Some(data)) => {
                    let parsed: f32 = qual.parse().map_err(|_| http_headers::Error::invalid())?;
                    let quality = (parsed * 1000_f32) as u16;

                    Ok(QualityMeta {
                        data,
                        quality,
                        _marker: PhantomData,
                    })
                }
                // No deliter present, assign a quality value of 1
                (Some(data), None) => Ok(QualityMeta {
                    data,
                    quality: 1000_u16,
                    _marker: PhantomData,
                }),
                _ => Err(http_headers::Error::invalid()),
            }
        }
    }

    impl<Delm: QualityDelimiter + Ord> QualityValue<Delm> {
        pub fn iter(&self) -> impl Iterator<Item = &str> {
            self.csv
                .iter()
                .map(|v| QualityMeta::<Delm>::try_from(v).unwrap())
                .into_iter()
                .sorted()
                .map(|pair| pair.data)
        }
    }

    impl<Delm: QualityDelimiter> From<FlatCsv> for QualityValue<Delm> {
        fn from(csv: FlatCsv) -> Self {
            QualityValue {
                csv,
                _marker: PhantomData,
            }
        }
    }

    impl<Delm: QualityDelimiter, F: Into<f32>> TryFrom<(&str, F)> for QualityValue<Delm> {
        type Error = http_headers::Error;

        fn try_from(pair: (&str, F)) -> Result<Self, Self::Error> {
            let value = HeaderValue::try_from(format!("{}{}{}", pair.0, Delm::STR, pair.1.into()))
                .map_err(|_| http_headers::Error::invalid())?;
            Ok(QualityValue {
                csv: value.into(),
                _marker: PhantomData,
            })
        }
    }

    impl<Delm> From<HeaderValue> for QualityValue<Delm> {
        fn from(value: HeaderValue) -> Self {
            QualityValue {
                csv: value.into(),
                _marker: PhantomData,
            }
        }
    }

    impl<'a, Delm> From<&'a QualityValue<Delm>> for HeaderValue {
        fn from(qual: &'a QualityValue<Delm>) -> HeaderValue {
            qual.csv.value.clone()
        }
    }

    impl<Delm> From<QualityValue<Delm>> for HeaderValue {
        fn from(qual: QualityValue<Delm>) -> HeaderValue {
            qual.csv.value
        }
    }

    impl<Delm: QualityDelimiter> TryFromValues for QualityValue<Delm> {
        fn try_from_values<'i, I>(values: &mut I) -> Result<Self, http_headers::Error>
        where
            I: Iterator<Item = &'i HeaderValue>,
        {
            let flat: FlatCsv = values.collect();
            Ok(QualityValue::from(flat))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::HeaderValue;
    use super::{
        sealed::{SemiLevel, SemiQ},
        QualityValue,
    };

    #[test]
    fn multiple_qualities() {
        let val = HeaderValue::from_static("gzip;q=1, br;q=0.8");
        let qual = QualityValue::<SemiQ>::from(val);

        let mut values = qual.iter();
        assert_eq!(values.next(), Some("gzip"));
        assert_eq!(values.next(), Some("br"));
        assert_eq!(values.next(), None);
    }

    #[test]
    fn multiple_qualities_wrong_order() {
        let val = HeaderValue::from_static("br;q=0.8, gzip;q=1.0");
        let qual = QualityValue::<SemiQ>::from(val);

        let mut values = qual.iter();
        assert_eq!(values.next(), Some("gzip"));
        assert_eq!(values.next(), Some("br"));
        assert_eq!(values.next(), None);
    }

    #[test]
    fn multiple_values() {
        let val = HeaderValue::from_static("deflate, gzip;q=1, br;q=0.8");
        let qual = QualityValue::<SemiQ>::from(val);

        let mut values = qual.iter();
        assert_eq!(values.next(), Some("deflate"));
        assert_eq!(values.next(), Some("gzip"));
        assert_eq!(values.next(), Some("br"));
        assert_eq!(values.next(), None);
    }

    #[test]
    fn multiple_values_wrong_order() {
        let val = HeaderValue::from_static("deflate, br;q=0.8, gzip;q=1, *;q=0.1");
        let qual = QualityValue::<SemiQ>::from(val);

        let mut values = qual.iter();
        assert_eq!(values.next(), Some("deflate"));
        assert_eq!(values.next(), Some("gzip"));
        assert_eq!(values.next(), Some("br"));
        assert_eq!(values.next(), Some("*"));
        assert_eq!(values.next(), None);
    }

    #[test]
    fn alternate_delimiter() {
        let val = HeaderValue::from_static("deflate, br;level=0.8, gzip;level=1");
        let qual = QualityValue::<SemiLevel>::from(val);

        let mut values = qual.iter();
        assert_eq!(values.next(), Some("deflate"));
        assert_eq!(values.next(), Some("gzip"));
        assert_eq!(values.next(), Some("br"));
        assert_eq!(values.next(), None);
    }
}
