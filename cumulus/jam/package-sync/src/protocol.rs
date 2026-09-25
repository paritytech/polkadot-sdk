// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! The package-info request/response protocol name and its network configuration.

use sc_network::{request_responses::IncomingRequest, service::traits::NetworkBackend};
use sp_runtime::traits::Block as BlockT;
use std::time::Duration;

/// Maximum request size. A request only carries a 32-byte block hash, plus the SCALE enum/struct
/// overhead.
pub const MAX_REQUEST_SIZE: u64 = 64;

/// Maximum response size. The response carries the `authorization`, the prerequisites and the PoV
/// spec — all bounded by the JAM package limits.
pub const MAX_RESPONSE_SIZE: u64 = 8 * 1024;

/// Request timeout.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Inbound channel size.
pub const INBOUND_CHANNEL_SIZE: usize = 64;

/// The protocol name, derived from the genesis hash and fork id.
///
/// Format: `/{hex genesis}[/{fork}]/jam-package-info/1`.
pub fn protocol_name<H: AsRef<[u8]>>(genesis_hash: H, fork_id: Option<&str>) -> String {
	let genesis_hash = genesis_hash.as_ref();
	if let Some(fork_id) = fork_id {
		format!("/{}/{}/jam-package-info/1", array_bytes::bytes2hex("", genesis_hash), fork_id)
	} else {
		format!("/{}/jam-package-info/1", array_bytes::bytes2hex("", genesis_hash))
	}
}

/// Build the request/response protocol configuration, together with the receiver the protocol
/// handler reads incoming requests from.
///
/// Mirrors `cumulus/client/bootnodes/src/config.rs`.
pub fn request_response_config<
	H: AsRef<[u8]>,
	B: BlockT,
	N: NetworkBackend<B, <B as BlockT>::Hash>,
>(
	genesis_hash: H,
	fork_id: Option<&str>,
) -> (N::RequestResponseProtocolConfig, async_channel::Receiver<IncomingRequest>) {
	let (inbound_tx, inbound_rx) = async_channel::bounded(INBOUND_CHANNEL_SIZE);

	let config = N::request_response_config(
		protocol_name(genesis_hash, fork_id).into(),
		Vec::new(),
		MAX_REQUEST_SIZE,
		MAX_RESPONSE_SIZE,
		REQUEST_TIMEOUT,
		Some(inbound_tx),
	);

	(config, inbound_rx)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn protocol_name_includes_genesis_and_optional_fork() {
		let genesis = [0xabu8; 32];
		let hex = "ab".repeat(32);

		assert_eq!(protocol_name(genesis, None), format!("/{hex}/jam-package-info/1"));
		assert_eq!(protocol_name(genesis, Some("fork")), format!("/{hex}/fork/jam-package-info/1"),);
	}
}
