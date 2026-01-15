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

//! Protocol name.

use std::{
    fmt::Display,
    hash::{Hash, Hasher},
    sync::Arc,
};

/// Protocol name.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "fuzz", derive(serde::Serialize, serde::Deserialize))]
pub enum ProtocolName {
    #[cfg(not(feature = "fuzz"))]
    Static(&'static str),
    Allocated(Arc<str>),
}

#[cfg(not(feature = "fuzz"))]
impl From<&'static str> for ProtocolName {
    fn from(protocol: &'static str) -> Self {
        ProtocolName::Static(protocol)
    }
}
#[cfg(feature = "fuzz")]
impl From<&'static str> for ProtocolName {
    fn from(protocol: &'static str) -> Self {
        ProtocolName::Allocated(Arc::from(protocol.to_string()))
    }
}

impl Display for ProtocolName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(not(feature = "fuzz"))]
            Self::Static(protocol) => protocol.fmt(f),
            Self::Allocated(protocol) => protocol.fmt(f),
        }
    }
}

impl From<String> for ProtocolName {
    fn from(protocol: String) -> Self {
        ProtocolName::Allocated(Arc::from(protocol))
    }
}

impl From<Arc<str>> for ProtocolName {
    fn from(protocol: Arc<str>) -> Self {
        Self::Allocated(protocol)
    }
}

impl std::ops::Deref for ProtocolName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        match self {
            #[cfg(not(feature = "fuzz"))]
            Self::Static(protocol) => protocol,
            Self::Allocated(protocol) => protocol,
        }
    }
}

impl Hash for ProtocolName {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self as &str).hash(state)
    }
}

impl PartialEq for ProtocolName {
    fn eq(&self, other: &Self) -> bool {
        (self as &str) == (other as &str)
    }
}

impl Eq for ProtocolName {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_protocol() {
        let protocol1 = ProtocolName::from(Arc::from(String::from("/protocol/1")));
        let protocol2 = ProtocolName::from("/protocol/1");

        assert_eq!(protocol1, protocol2);
    }
}
