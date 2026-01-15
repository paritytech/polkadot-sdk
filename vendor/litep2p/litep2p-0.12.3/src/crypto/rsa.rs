// Copyright 2025 litep2p developers
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

//! RSA public key.

use crate::error::ParseError;
use ring::signature::{UnparsedPublicKey, RSA_PKCS1_2048_8192_SHA256};
use x509_parser::{prelude::FromDer, x509::SubjectPublicKeyInfo};

/// An RSA public key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicKey(Vec<u8>);

impl PublicKey {
    /// Decode an RSA public key from a DER-encoded X.509 SubjectPublicKeyInfo structure.
    pub fn try_decode_x509(spki: &[u8]) -> Result<Self, ParseError> {
        SubjectPublicKeyInfo::from_der(spki)
            .map(|(_, spki)| Self(spki.subject_public_key.as_ref().to_vec()))
            .map_err(|_| ParseError::InvalidPublicKey)
    }

    /// Verify the RSA signature on a message using the public key.
    pub fn verify(&self, msg: &[u8], sig: &[u8]) -> bool {
        let key = UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, &self.0);
        key.verify(msg, sig).is_ok()
    }
}
