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

//! Bandwidth sinks for metering inbound/outbound bytes.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

/// Inner bandwidth sink
#[derive(Debug)]
struct InnerBandwidthSink {
    /// Number of inbound bytes.
    inbound: AtomicUsize,

    /// Number of outbound bytes.
    outbound: AtomicUsize,
}

/// Bandwidth sink which provides metering for inbound/outbound byte usage.
///
/// The reported values are not necessarily up to date with the latest information
/// and should not be used for metrics that require high precision but they do provide
/// an overall view of the data usage of `litep2p`.
#[derive(Debug, Clone)]
pub struct BandwidthSink(Arc<InnerBandwidthSink>);

impl BandwidthSink {
    /// Create new [`BandwidthSink`].
    pub(crate) fn new() -> Self {
        Self(Arc::new(InnerBandwidthSink {
            inbound: AtomicUsize::new(0usize),
            outbound: AtomicUsize::new(0usize),
        }))
    }

    /// Increase the amount of inbound bytes.
    pub(crate) fn increase_inbound(&self, bytes: usize) {
        let _ = self.0.inbound.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Increse the amount of outbound bytes.
    pub(crate) fn increase_outbound(&self, bytes: usize) {
        let _ = self.0.outbound.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Get total the number of bytes received.
    pub fn inbound(&self) -> usize {
        self.0.inbound.load(Ordering::Relaxed)
    }

    /// Get total the nubmer of bytes sent.
    pub fn outbound(&self) -> usize {
        self.0.outbound.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_bandwidth() {
        let sink = BandwidthSink::new();

        sink.increase_inbound(1337usize);
        sink.increase_outbound(1338usize);

        assert_eq!(sink.inbound(), 1337usize);
        assert_eq!(sink.outbound(), 1338usize);
    }
}
