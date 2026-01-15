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

//! Types used by [`Litep2p`](`crate::Litep2p`) protocols/transport.

use rand::Rng;

// Re-export the types used in public interfaces.
pub mod multiaddr {
    pub use multiaddr::{Error, Iter, Multiaddr, Onion3Addr, Protocol};
}
pub mod multihash {
    pub use multihash::{Code, Error, Multihash, MultihashDigest};
}
pub mod cid {
    pub use cid::{multihash::Multihash, Cid, CidGeneric, Error, Result, Version};
}

pub mod protocol;

/// Substream ID.
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct SubstreamId(usize);

impl Default for SubstreamId {
    fn default() -> Self {
        Self::new()
    }
}

impl SubstreamId {
    /// Create new [`SubstreamId`].
    pub fn new() -> Self {
        SubstreamId(0usize)
    }

    /// Get [`SubstreamId`] from a number that can be converted into a `usize`.
    pub fn from<T: Into<usize>>(value: T) -> Self {
        SubstreamId(value.into())
    }
}

/// Request ID.
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "fuzz", derive(serde::Serialize, serde::Deserialize))]
pub struct RequestId(usize);

impl RequestId {
    /// Get [`RequestId`] from a number that can be converted into a `usize`.
    pub fn from<T: Into<usize>>(value: T) -> Self {
        RequestId(value.into())
    }
}

/// Connection ID.
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct ConnectionId(usize);

impl ConnectionId {
    /// Create new [`ConnectionId`].
    pub fn new() -> Self {
        ConnectionId(0usize)
    }

    /// Generate random `ConnectionId`.
    pub fn random() -> Self {
        ConnectionId(rand::thread_rng().gen::<usize>())
    }
}

impl Default for ConnectionId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<usize> for ConnectionId {
    fn from(value: usize) -> Self {
        ConnectionId(value)
    }
}
