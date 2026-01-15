// Copyright (c) 2018-2019 Parity Technologies (UK) Ltd.
//
// Licensed under the Apache License, Version 2.0 or MIT license, at your option.
//
// A copy of the Apache License, Version 2.0 is included in the software as
// LICENSE-APACHE and a copy of the MIT license is included in the software
// as LICENSE-MIT. You may also obtain a copy of the Apache License, Version 2.0
// at https://www.apache.org/licenses/LICENSE-2.0 and a copy of the MIT license
// at https://opensource.org/licenses/MIT.

//! This crate implements the [Yamux specification][1].
//!
//! It multiplexes independent I/O streams over reliable, ordered connections,
//! such as TCP/IP.
//!
//! The three primary objects, clients of this crate interact with, are:
//!
//! - [`Connection`], which wraps the underlying I/O resource, e.g. a socket,
//! - [`Stream`], which implements [`futures::io::AsyncRead`] and [`futures::io::AsyncWrite`], and
//! - [`Control`], to asynchronously control the [`Connection`].
//!
//! [1]: https://github.com/hashicorp/yamux/blob/master/spec.md

#![forbid(unsafe_code)]

mod control;

pub use yamux::{
    Config, Connection, ConnectionError, FrameDecodeError, HeaderDecodeError, Mode, Packet, Result,
    Stream, StreamId,
};

// Switching to the "poll" based yamux API is a massive breaking change for litep2p.
// Instead, we rely on the upstream yamux and keep the old controller API.
pub use crate::yamux::control::{Control, ControlledConnection};

pub const DEFAULT_CREDIT: u32 = 256 * 1024; // as per yamux specification

/// The maximum number of streams we will open without an acknowledgement from the other peer.
///
/// This enables a very basic form of backpressure on the creation of streams.
const MAX_ACK_BACKLOG: usize = 256;
