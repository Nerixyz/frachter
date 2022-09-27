pub mod mime {
    use std::fmt::Formatter;
    use serde::{Deserializer, Serializer};
    use serde::de::{Error, Visitor};

    pub fn deserialize<'de, D>(de: D) -> Result<mime::Mime, D::Error> where D: Deserializer<'de> {
        struct Vis;
        impl Visitor<'_> for Vis {
            type Value = mime::Mime;

            fn expecting(&self, f: &mut Formatter) -> std::fmt::Result {
                write!(f, "a content type (mime)")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E> where E: Error {
                v.parse().map_err(|e| E::custom(e))
            }
        }
        de.deserialize_str(Vis)
    }

    #[allow(unused)]
    pub fn serialize<S>(mime: &mime::Mime, ser: S) -> Result<S::Ok, S::Error> where S: Serializer {
        ser.serialize_str(mime.as_ref())
    }
}