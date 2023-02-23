use async_trait::async_trait;
use std::hash::Hash;

// #[async_trait]
// pub trait DeserializeAsync<V> {
//     type Error: std::error::Error;

//     fn deserialize_from<R>(&self, reader: &mut R) -> Result<V, Self::Error>
//     where
//         // V: serde::Deserialize<'de>,
//         R: tokio::io::AsyncRead;
// }

// pub trait SerializeAsync<V> {
//     type Error: std::error::Error;

//     fn serialize_to<W>(&self, value: &V, writer: &mut W) -> Result<(), Self::Error>
//     where
//         //     V: serde::Serialize,
//         W: tokio::io::AsyncWrite;
// }

#[derive(Default, Debug)]
pub struct MessagePack {}

impl<'de, V> super::Deserialize<V> for MessagePack
where
    V: serde::Deserialize<'de>,
{
    type Error = rmp_serde::decode::Error;

    fn deserialize_from<R>(&self, reader: &mut R) -> Result<V, super::Error<Self::Error>>
    where
        R: std::io::Read,
    {
        V::deserialize(&mut rmp_serde::Deserializer::new(reader)).map_err(super::Error::Test)
    }
}

impl<V> super::Serialize<V> for MessagePack
where
    V: serde::Serialize + Sync,
{
    type Error = rmp_serde::encode::Error;

    fn serialize_to<W>(&self, value: &V, writer: &mut W) -> Result<(), super::Error<Self::Error>>
    where
        W: std::io::Write,
    {
        value
            .serialize(&mut rmp_serde::Serializer::new(writer))
            .map_err(super::Error::Test)
    }
}
