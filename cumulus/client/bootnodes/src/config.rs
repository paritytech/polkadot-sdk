// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Parachain bootnode request-response protocol configuration.

use sc_network::{
	request_responses::IncomingRequest, service::traits::NetworkBackend, ProtocolName,
};
use sp_runtime::traits::Block as BlockT;
use std::time::Duration;

/// Maximum number of addresses allowed in the response.
pub const MAX_ADDRESSES: usize = 32;
/// Expected maximum number of simultaneous requests from remote peers.
/// Should be enough for a testnet with a plenty of nodes starting at the same time.
const INBOUND_CHANNEL_SIZE: usize = 1000;
/// Maximum request size. Should be enough to fit SCALE-compact-encoded `para_id`.
const MAX_REQUEST_SIZE: u64 = 128;
/// Maximum response size as per RFC.
const MAX_RESPONSE_SIZE: u64 = 16 * 1024;
/// Request-response protocol timeout.
const TIMEOUT: Duration = Duration::from_secs(20);

/// Bootnode request-response protocol name given a genesis hash and fork id.
pub fn paranode_protocol_name<Hash: AsRef<[u8]>>(
	genesis_hash: Hash,
	fork_id: Option<&str>,
) -> ProtocolName {
	let genesis_hash = genesis_hash.as_ref();
	if let Some(fork_id) = fork_id {
		// This is not stated in RFC-0008, but other polkadot protocol names are based on `fork_id`
		// if it is present, so we also use it here.
		format!("/{}/{}/paranode", array_bytes::bytes2hex("", genesis_hash), fork_id)
	} else {
		format!("/{}/paranode", array_bytes::bytes2hex("", genesis_hash))
	}
	.into()
}

/// Bootnode request-response protocol config.
pub fn bootnode_request_response_config<
	Hash: AsRef<[u8]>,
	B: BlockT,
	N: NetworkBackend<B, <B as BlockT>::Hash>,
>(
	genesis_hash: Hash,
	fork_id: Option<&str>,
) -> (N::RequestResponseProtocolConfig, async_channel::Receiver<IncomingRequest>) {
	let (inbound_tx, inbound_rx) = async_channel::bounded(INBOUND_CHANNEL_SIZE);

	let config = N::request_response_config(
		paranode_protocol_name(genesis_hash, fork_id),
		Vec::new(),
		MAX_REQUEST_SIZE,
		MAX_RESPONSE_SIZE,
		TIMEOUT,
		Some(inbound_tx),
	);

	(config, inbound_rx)
}
