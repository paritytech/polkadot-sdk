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

use crate::{codec::unsigned_varint::UnsignedVarint, error::ParseError, transport::webrtc::schema};

use prost::Message;
use tokio_util::codec::{Decoder, Encoder};

/// WebRTC mesage.
#[derive(Debug)]
pub struct WebRtcMessage {
    /// Payload.
    pub payload: Option<Vec<u8>>,

    // Flags.
    pub flags: Option<i32>,
}

impl WebRtcMessage {
    /// Encode WebRTC message.
    pub fn encode(payload: Vec<u8>) -> Vec<u8> {
        let protobuf_payload = schema::webrtc::Message {
            message: (!payload.is_empty()).then_some(payload),
            flag: None,
        };
        let mut payload = Vec::with_capacity(protobuf_payload.encoded_len());
        protobuf_payload
            .encode(&mut payload)
            .expect("Vec<u8> to provide needed capacity");

        let mut out_buf = bytes::BytesMut::with_capacity(payload.len() + 4);
        let mut codec = UnsignedVarint::new(None);
        let _result = codec.encode(payload.into(), &mut out_buf);

        out_buf.into()
    }

    /// Encode WebRTC message with flags.
    #[allow(unused)]
    pub fn encode_with_flags(payload: Vec<u8>, flags: i32) -> Vec<u8> {
        let protobuf_payload = schema::webrtc::Message {
            message: (!payload.is_empty()).then_some(payload),
            flag: Some(flags),
        };
        let mut payload = Vec::with_capacity(protobuf_payload.encoded_len());
        protobuf_payload
            .encode(&mut payload)
            .expect("Vec<u8> to provide needed capacity");

        let mut out_buf = bytes::BytesMut::with_capacity(payload.len() + 4);
        let mut codec = UnsignedVarint::new(None);
        let _result = codec.encode(payload.into(), &mut out_buf);

        out_buf.into()
    }

    /// Decode payload into [`WebRtcMessage`].
    pub fn decode(payload: &[u8]) -> Result<Self, ParseError> {
        // TODO: https://github.com/paritytech/litep2p/issues/352 set correct size
        let mut codec = UnsignedVarint::new(None);
        let mut data = bytes::BytesMut::from(payload);
        let result = codec
            .decode(&mut data)
            .map_err(|_| ParseError::InvalidData)?
            .ok_or(ParseError::InvalidData)?;

        match schema::webrtc::Message::decode(result) {
            Ok(message) => Ok(Self {
                payload: message.message,
                flags: message.flag,
            }),
            Err(_) => Err(ParseError::InvalidData),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_payload_no_flags() {
        let message = WebRtcMessage::encode("Hello, world!".as_bytes().to_vec());
        let decoded = WebRtcMessage::decode(&message).unwrap();

        assert_eq!(decoded.payload, Some("Hello, world!".as_bytes().to_vec()));
        assert_eq!(decoded.flags, None);
    }

    #[test]
    fn with_payload_and_flags() {
        let message = WebRtcMessage::encode_with_flags("Hello, world!".as_bytes().to_vec(), 1i32);
        let decoded = WebRtcMessage::decode(&message).unwrap();

        assert_eq!(decoded.payload, Some("Hello, world!".as_bytes().to_vec()));
        assert_eq!(decoded.flags, Some(1i32));
    }

    #[test]
    fn no_payload_with_flags() {
        let message = WebRtcMessage::encode_with_flags(vec![], 2i32);
        let decoded = WebRtcMessage::decode(&message).unwrap();

        assert_eq!(decoded.payload, None);
        assert_eq!(decoded.flags, Some(2i32));
    }
}
