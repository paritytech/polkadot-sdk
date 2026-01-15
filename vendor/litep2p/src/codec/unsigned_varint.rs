// Copyright 2023 litep2p developers
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

//! [`unsigned-varint`](https://github.com/multiformats/unsigned-varint) codec.

use crate::error::Error;

use bytes::{Bytes, BytesMut};
use tokio_util::codec::{Decoder, Encoder};
use unsigned_varint::codec::UviBytes;

/// Unsigned varint codec.
pub struct UnsignedVarint {
    codec: UviBytes<bytes::Bytes>,
}

impl UnsignedVarint {
    /// Create new [`UnsignedVarint`] codec.
    pub fn new(max_size: Option<usize>) -> Self {
        let mut codec = UviBytes::<Bytes>::default();

        if let Some(max_size) = max_size {
            codec.set_max_len(max_size);
        }

        Self { codec }
    }

    /// Set maximum size for encoded/decodes values.
    pub fn with_max_size(max_size: usize) -> Self {
        let mut codec = UviBytes::<Bytes>::default();
        codec.set_max_len(max_size);

        Self { codec }
    }

    /// Encode `payload` using `unsigned-varint`.
    pub fn encode<T: Into<Bytes>>(payload: T) -> crate::Result<Vec<u8>> {
        let payload: Bytes = payload.into();

        assert!(payload.len() <= u32::MAX as usize);

        let mut bytes = BytesMut::with_capacity(payload.len() + 4);
        let mut codec = Self::new(None);
        codec.encode(payload, &mut bytes)?;

        Ok(bytes.into())
    }

    /// Decode `payload` into `BytesMut`.
    pub fn decode(payload: &mut BytesMut) -> crate::Result<BytesMut> {
        UviBytes::<Bytes>::default().decode(payload)?.ok_or(Error::InvalidData)
    }
}

impl Decoder for UnsignedVarint {
    type Item = BytesMut;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.codec.decode(src).map_err(From::from)
    }
}

impl Encoder<Bytes> for UnsignedVarint {
    type Error = Error;

    fn encode(&mut self, item: Bytes, dst: &mut bytes::BytesMut) -> Result<(), Self::Error> {
        self.codec.encode(item, dst).map_err(From::from)
    }
}

#[cfg(test)]
mod tests {
    use super::{Bytes, BytesMut, UnsignedVarint};

    #[test]
    fn max_size_respected() {
        let mut codec = UnsignedVarint::with_max_size(1024);

        {
            use tokio_util::codec::Encoder;

            let bytes_to_encode: Bytes = vec![0u8; 1024].into();
            let mut out_bytes = BytesMut::with_capacity(2048);
            assert!(codec.encode(bytes_to_encode, &mut out_bytes).is_ok());
        }

        {
            use tokio_util::codec::Encoder;

            let bytes_to_encode: Bytes = vec![1u8; 1025].into();
            let mut out_bytes = BytesMut::with_capacity(2048);
            assert!(codec.encode(bytes_to_encode, &mut out_bytes).is_err());
        }
    }

    #[test]
    fn encode_decode_works() {
        let encoded1 = UnsignedVarint::encode(vec![0u8; 512]).unwrap();
        let mut encoded2 = {
            use tokio_util::codec::Encoder;

            let mut codec = UnsignedVarint::with_max_size(512);
            let bytes_to_encode: Bytes = vec![0u8; 512].into();
            let mut out_bytes = BytesMut::with_capacity(2048);
            codec.encode(bytes_to_encode, &mut out_bytes).unwrap();
            out_bytes
        };

        assert_eq!(encoded1, encoded2);

        let decoded1 = UnsignedVarint::decode(&mut encoded2).unwrap();
        let decoded2 = {
            use tokio_util::codec::Decoder;

            let mut codec = UnsignedVarint::with_max_size(512);
            let mut encoded1 = BytesMut::from(&encoded1[..]);
            codec.decode(&mut encoded1).unwrap().unwrap()
        };

        assert_eq!(decoded1, decoded2);
    }
}
