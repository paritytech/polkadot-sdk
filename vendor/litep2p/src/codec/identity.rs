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

//! Identity codec that reads/writes `N` bytes from/to source/sink.

use crate::error::Error;

use bytes::{BufMut, Bytes, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

/// Identity codec.
pub struct Identity {
    payload_len: usize,
}

impl Identity {
    /// Create new [`Identity`] codec.
    pub fn new(payload_len: usize) -> Self {
        assert!(payload_len != 0);

        Self { payload_len }
    }

    /// Encode `payload` using identity codec.
    pub fn encode<T: Into<Bytes>>(payload: T) -> crate::Result<Vec<u8>> {
        let payload: Bytes = payload.into();
        Ok(payload.into())
    }
}

impl Decoder for Identity {
    type Item = BytesMut;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.is_empty() || src.len() < self.payload_len {
            return Ok(None);
        }

        Ok(Some(src.split_to(self.payload_len)))
    }
}

impl Encoder<Bytes> for Identity {
    type Error = Error;

    fn encode(&mut self, item: Bytes, dst: &mut bytes::BytesMut) -> Result<(), Self::Error> {
        if item.len() > self.payload_len || item.is_empty() {
            return Err(Error::InvalidData);
        }

        dst.put_slice(item.as_ref());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_works() {
        let mut codec = Identity::new(48);
        let mut out_buf = BytesMut::with_capacity(32);
        let bytes = Bytes::from(vec![0u8; 48]);

        assert!(codec.encode(bytes.clone(), &mut out_buf).is_ok());
        assert_eq!(out_buf.freeze(), bytes);
    }

    #[test]
    fn decoding_works() {
        let mut codec = Identity::new(64);
        let bytes = vec![3u8; 64];
        let copy = bytes.clone();
        let mut bytes = BytesMut::from(&bytes[..]);

        let decoded = codec.decode(&mut bytes).unwrap().unwrap();
        assert_eq!(decoded, copy);
    }

    #[test]
    fn decoding_smaller_payloads() {
        let mut codec = Identity::new(100);
        let bytes = [3u8; 64];
        let mut bytes = BytesMut::from(&bytes[..]);

        assert!(codec.decode(&mut bytes).unwrap().is_none());
    }

    #[test]
    fn empty_encode() {
        let mut codec = Identity::new(32);
        let mut out_buf = BytesMut::with_capacity(32);
        assert!(codec.encode(Bytes::new(), &mut out_buf).is_err());
    }

    #[test]
    fn decode_encode() {
        let mut codec = Identity::new(32);
        assert!(codec.decode(&mut BytesMut::new()).unwrap().is_none());
    }

    #[test]
    fn direct_encoding_works() {
        assert_eq!(
            Identity::encode(vec![1, 3, 3, 7]).unwrap(),
            vec![1, 3, 3, 7]
        );
    }

    #[test]
    #[should_panic]
    #[cfg(debug_assertions)]
    fn empty_identity_codec() {
        let _codec = Identity::new(0usize);
    }
}
