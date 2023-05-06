// Copyright 2021 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

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

//! Cumulus timestamp related primitives.
//!
//! Provides a [`InherentDataProvider`] that should be used in the validation phase of the parachain.
//! It will be used to create the inherent data and that will be used to check the inherents inside
//! the parachain block (in this case the timestamp inherent). As we don't have access to any clock
//! from the runtime the timestamp is always passed as an inherent into the runtime. To check this
//! inherent when validating the block, we will use the relay chain slot. As the relay chain slot
//! is derived from a timestamp, we can easily convert it back to a timestamp by muliplying it with
//! the slot duration. By comparing the relay chain slot derived timestamp with the timestamp we can
//! ensure that the parachain timestamp is reasonable.

#![cfg_attr(not(feature = "std"), no_std)]

use cumulus_primitives_core::relay_chain::Slot;
use sp_inherents::{Error, InherentData};
use sp_std::time::Duration;

pub use sp_timestamp::{InherentType, INHERENT_IDENTIFIER};

/// The inherent data provider for the timestamp.
///
/// This should be used in the runtime when checking the inherents in the validation phase of the
/// parachain.
pub struct InherentDataProvider {
	relay_chain_slot: Slot,
	relay_chain_slot_duration: Duration,
}

impl InherentDataProvider {
	/// Create `Self` from the given relay chain slot and slot duration.
	pub fn from_relay_chain_slot_and_duration(
		relay_chain_slot: Slot,
		relay_chain_slot_duration: Duration,
	) -> Self {
		Self { relay_chain_slot, relay_chain_slot_duration }
	}

	/// Create the inherent data.
	pub fn create_inherent_data(&self) -> Result<InherentData, Error> {
		let mut inherent_data = InherentData::new();
		self.provide_inherent_data(&mut inherent_data).map(|_| inherent_data)
	}

	/// Provide the inherent data into the given `inherent_data`.
	pub fn provide_inherent_data(&self, inherent_data: &mut InherentData) -> Result<(), Error> {
		// As the parachain starts building at around `relay_chain_slot + 1` we use that slot to
		// calculate the timestamp.
		let data: InherentType = ((*self.relay_chain_slot + 1) *
			self.relay_chain_slot_duration.as_millis() as u64)
			.into();

		inherent_data.put_data(INHERENT_IDENTIFIER, &data)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	use codec::{Decode, Encode};
	use cumulus_primitives_core::{relay_chain::Hash as PHash, PersistedValidationData};
	use cumulus_test_client::{
		runtime::{Block, Header, WASM_BINARY},
		BlockData, BuildParachainBlockData, Client, ClientBlockImportExt, ExecutorResult, HeadData,
		InitBlockBuilder, ParachainBlockData, TestClientBuilder, TestClientBuilderExt,
		ValidationParams,
	};
	use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;
	use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
	use std::{env, process::Command, str::FromStr};

	const SLOT_DURATION: u64 = 6000;

	fn call_validate_block(
		parent_head: Header,
		block_data: ParachainBlockData,
		relay_parent_storage_root: PHash,
	) -> ExecutorResult<Header> {
		cumulus_test_client::validate_block(
			ValidationParams {
				block_data: BlockData(block_data.encode()),
				parent_head: HeadData(parent_head.encode()),
				relay_parent_number: 1,
				relay_parent_storage_root,
			},
			WASM_BINARY.expect("You need to build the WASM binaries to run the tests!"),
		)
		.map(|v| Header::decode(&mut &v.head_data.0[..]).expect("Decodes `Header`."))
	}

	fn build_block(
		client: &Client,
		hash: <Block as BlockT>::Hash,
		timestamp: u64,
		relay_chain_slot: Slot,
	) -> (ParachainBlockData, PHash) {
		let sproof_builder =
			RelayStateSproofBuilder { current_slot: relay_chain_slot, ..Default::default() };

		let parent_header = client.header(hash).ok().flatten().expect("Genesis header exists");

		let relay_parent_storage_root = sproof_builder.clone().into_state_root_and_proof().0;

		let validation_data = PersistedValidationData {
			relay_parent_number: 1,
			parent_head: parent_header.encode().into(),
			..Default::default()
		};

		let block = client
			.init_block_builder_with_timestamp(
				hash,
				Some(validation_data),
				sproof_builder,
				timestamp,
			)
			.build_parachain_block(*parent_header.state_root());

		(block, relay_parent_storage_root)
	}

	#[test]
	fn check_timestamp_inherent_works() {
		sp_tracing::try_init_simple();
		let relay_chain_slot = 2;

		if env::var("RUN_TEST").is_ok() {
			let mut client = TestClientBuilder::default().build();
			let timestamp = u64::from_str(&env::var("TIMESTAMP").expect("TIMESTAMP is set"))
				.expect("TIMESTAMP is a valid `u64`");

			let block =
				build_block(&client, client.chain_info().genesis_hash, SLOT_DURATION, 1.into())
					.0
					.into_block();
			futures::executor::block_on(
				client.import(sp_consensus::BlockOrigin::Own, block.clone()),
			)
			.unwrap();

			let hashof1 = block.hash();
			let (block, relay_chain_root) =
				build_block(&client, hashof1, timestamp, relay_chain_slot.into());

			let header = call_validate_block(
				client.header(hashof1).ok().flatten().expect("Genesis header exists"),
				block.clone(),
				relay_chain_root,
			)
			.expect("Calls validate block");
			assert_eq!(block.header(), &header);
		} else {
			let slot_timestamp = relay_chain_slot * SLOT_DURATION;

			for (timestamp, res) in &[
				(slot_timestamp, true),
				(slot_timestamp - 500, true),
				(slot_timestamp + 500, true),
				(slot_timestamp * 10, false),
			] {
				let output = Command::new(env::current_exe().unwrap())
					.args(["check_timestamp_inherent_works", "--", "--nocapture"])
					.env("RUN_TEST", "1")
					.env("TIMESTAMP", timestamp.to_string())
					.output()
					.expect("Runs the test");

				if !res {
					assert!(String::from_utf8(output.stderr)
						.unwrap()
						.contains("Checking inherents failed"));
				}

				assert!(dbg!(output.status.success()) == *res);
			}
		}
	}
}
