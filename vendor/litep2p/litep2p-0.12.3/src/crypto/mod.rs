// Copyright 2023 Protocol Labs.
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

//! Crypto-related code.

use crate::{error::ParseError, peer_id::*};

pub mod ed25519;
#[cfg(feature = "rsa")]
pub mod rsa;

pub(crate) mod noise;
#[cfg(feature = "quic")]
pub(crate) mod tls;
pub(crate) mod keys_proto {
    include!(concat!(env!("OUT_DIR"), "/keys_proto.rs"));
}

/// The public key of a node's identity keypair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublicKey {
    /// A public Ed25519 key.
    Ed25519(ed25519::PublicKey),
}

impl PublicKey {
    /// Encode the public key into a protobuf structure for storage or
    /// exchange with other nodes.
    pub fn to_protobuf_encoding(&self) -> Vec<u8> {
        use prost::Message;

        let public_key = keys_proto::PublicKey::from(self);

        let mut buf = Vec::with_capacity(public_key.encoded_len());
        public_key.encode(&mut buf).expect("Vec<u8> provides capacity as needed");
        buf
    }

    /// Convert the `PublicKey` into the corresponding `PeerId`.
    pub fn to_peer_id(&self) -> PeerId {
        self.into()
    }
}

impl From<&PublicKey> for keys_proto::PublicKey {
    fn from(key: &PublicKey) -> Self {
        match key {
            PublicKey::Ed25519(key) => keys_proto::PublicKey {
                r#type: keys_proto::KeyType::Ed25519 as i32,
                data: key.to_bytes().to_vec(),
            },
        }
    }
}

impl TryFrom<keys_proto::PublicKey> for PublicKey {
    type Error = ParseError;

    fn try_from(pubkey: keys_proto::PublicKey) -> Result<Self, Self::Error> {
        let key_type = keys_proto::KeyType::try_from(pubkey.r#type)
            .map_err(|_| ParseError::UnknownKeyType(pubkey.r#type))?;

        if key_type == keys_proto::KeyType::Ed25519 {
            Ok(ed25519::PublicKey::try_from_bytes(&pubkey.data).map(PublicKey::Ed25519)?)
        } else {
            Err(ParseError::UnknownKeyType(key_type as i32))
        }
    }
}

impl From<ed25519::PublicKey> for PublicKey {
    fn from(public_key: ed25519::PublicKey) -> Self {
        PublicKey::Ed25519(public_key)
    }
}

/// The public key of a remote node's identity keypair. Supports RSA keys additionally to ed25519.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemotePublicKey {
    /// A public Ed25519 key.
    Ed25519(ed25519::PublicKey),
    /// A public RSA key.
    #[cfg(feature = "rsa")]
    Rsa(rsa::PublicKey),
}

impl RemotePublicKey {
    /// Verify a signature for a message using this public key, i.e. check
    /// that the signature has been produced by the corresponding
    /// private key (authenticity), and that the message has not been
    /// tampered with (integrity).
    #[must_use]
    pub fn verify(&self, msg: &[u8], sig: &[u8]) -> bool {
        use RemotePublicKey::*;
        match self {
            Ed25519(pk) => pk.verify(msg, sig),
            #[cfg(feature = "rsa")]
            Rsa(pk) => pk.verify(msg, sig),
        }
    }

    /// Decode a public key from a protobuf structure, e.g. read from storage
    /// or received from another node.
    pub fn from_protobuf_encoding(bytes: &[u8]) -> Result<RemotePublicKey, ParseError> {
        use prost::Message;

        let pubkey = keys_proto::PublicKey::decode(bytes)?;

        pubkey.try_into()
    }
}

impl TryFrom<keys_proto::PublicKey> for RemotePublicKey {
    type Error = ParseError;

    fn try_from(pubkey: keys_proto::PublicKey) -> Result<Self, Self::Error> {
        let key_type = keys_proto::KeyType::try_from(pubkey.r#type)
            .map_err(|_| ParseError::UnknownKeyType(pubkey.r#type))?;

        match key_type {
            keys_proto::KeyType::Ed25519 =>
                ed25519::PublicKey::try_from_bytes(&pubkey.data).map(RemotePublicKey::Ed25519),
            #[cfg(feature = "rsa")]
            keys_proto::KeyType::Rsa =>
                rsa::PublicKey::try_decode_x509(&pubkey.data).map(RemotePublicKey::Rsa),
            _ => Err(ParseError::UnknownKeyType(key_type as i32)),
        }
    }
}
