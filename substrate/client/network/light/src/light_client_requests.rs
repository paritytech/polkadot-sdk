// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Helpers for outgoing and incoming light client requests.

use sc_network::{
	config::ProtocolId, request_responses::IncomingRequest, NetworkBackend, MAX_RESPONSE_SIZE,
};
use sp_runtime::traits::Block;

use std::time::Duration;

/// For incoming light client requests.
pub mod handler;

/// The version of the light client protocol under which all request types, including
/// `RemoteReadExtrinsicsRequest`, are served.
const PROTOCOL_VERSION: u32 = 3;

/// The previous version of the light client protocol, kept as a fallback for peers that only
/// know about the request types that predate `RemoteReadExtrinsicsRequest`.
const LEGACY_PROTOCOL_VERSION: u32 = 2;

/// Generate the light client protocol name from the genesis hash, fork id, and protocol version.
fn generate_protocol_name<Hash: AsRef<[u8]>>(
	genesis_hash: Hash,
	fork_id: Option<&str>,
	version: u32,
) -> String {
	let genesis_hash = genesis_hash.as_ref();
	if let Some(fork_id) = fork_id {
		format!("/{}/{}/light/{}", array_bytes::bytes2hex("", genesis_hash), fork_id, version)
	} else {
		format!("/{}/light/{}", array_bytes::bytes2hex("", genesis_hash), version)
	}
}

/// Generate the legacy light client protocol name from chain specific protocol identifier.
fn generate_legacy_protocol_name(protocol_id: &ProtocolId) -> String {
	format!("/{}/light/2", protocol_id.as_ref())
}

/// Generates a `RequestResponseProtocolConfig` for the light client request protocol, refusing
/// incoming requests.
pub fn generate_protocol_config<
	Hash: AsRef<[u8]>,
	B: Block,
	N: NetworkBackend<B, <B as Block>::Hash>,
>(
	protocol_id: &ProtocolId,
	genesis_hash: Hash,
	fork_id: Option<&str>,
	inbound_queue: async_channel::Sender<IncomingRequest>,
) -> N::RequestResponseProtocolConfig {
	N::request_response_config(
		generate_protocol_name(&genesis_hash, fork_id, PROTOCOL_VERSION).into(),
		vec![
			generate_protocol_name(&genesis_hash, fork_id, LEGACY_PROTOCOL_VERSION).into(),
			generate_legacy_protocol_name(protocol_id).into(),
		],
		1 * 1024 * 1024,
		MAX_RESPONSE_SIZE,
		Duration::from_secs(15),
		Some(inbound_queue),
	)
}
