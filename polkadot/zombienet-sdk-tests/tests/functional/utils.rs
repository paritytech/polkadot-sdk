// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use zombienet_sdk::subxt::{self, ext::scale_value::Value};

/// Creates a sudo call to trigger BEEFY ConsensusReset via `Beefy::set_new_genesis`.
pub fn create_set_new_genesis_call(delay_in_blocks: u32) -> subxt::tx::DynamicPayload {
	// Construct: Beefy(set_new_genesis { delay_in_blocks })
	let set_new_genesis = Value::named_variant(
		"set_new_genesis",
		[("delay_in_blocks", Value::u128(delay_in_blocks as u128))],
	);
	let beefy_call = Value::unnamed_variant("Beefy", [set_new_genesis]);

	subxt::tx::dynamic("Sudo", "sudo", vec![beefy_call])
}
