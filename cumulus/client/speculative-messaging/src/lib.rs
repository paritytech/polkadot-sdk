// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <http://www.gnu.org/licenses/>.

//! Client-side support for speculative cross-chain messaging.
//!
//! This crate provides the off-chain networking layer for exchanging
//! speculative messages between parachain collators via relay chain peers.
//!
//! # Architecture
//!
//! Messages flow through relay chain peers as intermediaries:
//!
//! ```text
//! CollatorA -> RelayPeerA -> RelayPeerB -> CollatorB
//! ```
//!
//! Each parachain collator is paired with a relay chain peer (often
//! an embedded relay chain node in the same process). When collator A
//! wants to send messages to collator B:
//!
//! 1. CollatorA hands the [`MessageBatch`] to its relay peer (PeerA).
//! 2. PeerA looks up PeerB (the relay peer for ParaB) in the
//!    [`PeerRegistry`] and forwards the batch.
//! 3. PeerB delivers the batch to CollatorB.
//!
//! # Discovery
//!
//! For the MVP, peer discovery is hardcoded: a [`HardcodedPeerRegistry`]
//! backed by in-memory configuration (populated from pallet storage via
//! the `set_relay_peer` / `remove_relay_peer` runtime calls on
//! `pallet-speculative-messaging`).
//!
//! # Components
//!
//! - [`protocol`]: Request/response message types for the forwarding
//!   protocol.
//! - [`registry`]: Peer discovery registry trait and implementations.
//! - [`service`]: The main worker that orchestrates message exchange.
//! - [`transport`]: Concrete [`NetworkTransport`] backed by
//!   [`sc_network`].
//! - [`outbound`]: Block-following outbound message distributor.
//! - [`node`]: Node-level wiring: protocol config and startup function.

pub mod error;
pub mod node;
pub mod outbound;
pub mod protocol;
pub mod registry;
pub mod service;
pub mod transport;

pub use error::Error;
pub use node::{spec_msg_request_response_config, start_speculative_messaging, SpecMsgHandle};
pub use protocol::{ForwardMessageRequest, ForwardMessageResponse, NodeRole, PROTOCOL_NAME};
pub use registry::{HardcodedPeerRegistry, OpaquePeerId, PeerRegistry};
pub use service::{IncomingRequest, NetworkTransport, ServiceConfig, SpeculativeMessagingWorker};
pub use transport::ScNetworkTransport;
