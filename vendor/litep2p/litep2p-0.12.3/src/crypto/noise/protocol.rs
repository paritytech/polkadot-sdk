// Copyright 2019 Parity Technologies (UK) Ltd.
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

use crate::crypto::noise::x25519_spec;

use rand::SeedableRng;
use zeroize::Zeroize;

/// DH keypair.
#[derive(Clone)]
pub struct Keypair<T: Zeroize> {
    pub secret: SecretKey<T>,
    pub public: PublicKey<T>,
}

/// DH secret key.
#[derive(Clone)]
pub struct SecretKey<T: Zeroize>(pub T);

impl<T: Zeroize> Drop for SecretKey<T> {
    fn drop(&mut self) {
        self.0.zeroize()
    }
}

impl<T: AsRef<[u8]> + Zeroize> AsRef<[u8]> for SecretKey<T> {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

/// DH public key.
#[derive(Clone)]
pub struct PublicKey<T>(pub T);

impl<T: AsRef<[u8]>> PartialEq for PublicKey<T> {
    fn eq(&self, other: &PublicKey<T>) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl<T: AsRef<[u8]>> Eq for PublicKey<T> {}

impl<T: AsRef<[u8]>> AsRef<[u8]> for PublicKey<T> {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

/// Custom `snow::CryptoResolver` which delegates to either the
/// `RingResolver` on native or the `DefaultResolver` on wasm
/// for hash functions and symmetric ciphers, while using x25519-dalek
/// for Curve25519 DH.
pub struct Resolver;

impl snow::resolvers::CryptoResolver for Resolver {
    fn resolve_rng(&self) -> Option<Box<dyn snow::types::Random>> {
        Some(Box::new(Rng(rand::rngs::StdRng::from_entropy())))
    }

    fn resolve_dh(&self, choice: &snow::params::DHChoice) -> Option<Box<dyn snow::types::Dh>> {
        if let snow::params::DHChoice::Curve25519 = choice {
            Some(Box::new(Keypair::<x25519_spec::X25519Spec>::default()))
        } else {
            None
        }
    }

    fn resolve_hash(
        &self,
        choice: &snow::params::HashChoice,
    ) -> Option<Box<dyn snow::types::Hash>> {
        snow::resolvers::RingResolver.resolve_hash(choice)
    }

    fn resolve_cipher(
        &self,
        choice: &snow::params::CipherChoice,
    ) -> Option<Box<dyn snow::types::Cipher>> {
        snow::resolvers::RingResolver.resolve_cipher(choice)
    }
}

/// Wrapper around a CSPRNG to implement `snow::Random` trait for.
struct Rng(rand::rngs::StdRng);

impl rand::RngCore for Rng {
    fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }

    fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.0.fill_bytes(dest)
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        self.0.try_fill_bytes(dest)
    }
}

impl rand::CryptoRng for Rng {}

impl snow::types::Random for Rng {}
