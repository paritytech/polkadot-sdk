// Copyright 2020 Parity Technologies (UK) Ltd.
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

use crate::Client;
use cumulus_primitives::{inherents::{VALIDATION_DATA_IDENTIFIER, ValidationDataType}, ValidationData};
use cumulus_test_runtime::GetLastTimestamp;
use polkadot_primitives::v1::BlockNumber as PBlockNumber;
use sc_block_builder::BlockBuilderApi;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_core::ExecutionContext;
use sp_runtime::generic::BlockId;

/// Generate the inherents required by the test runtime.
///
/// - `validation_data`: The [`ValidationData`] that will be passed as inherent
///                      data into the runtime when building the inherents. If
///                      `None` is passed, the default value will be used.
pub fn generate_block_inherents(
	client: &Client,
	validation_data: Option<ValidationData<PBlockNumber>>,
) -> Vec<cumulus_test_runtime::UncheckedExtrinsic> {
	let mut inherent_data = sp_inherents::InherentData::new();
	let block_id = BlockId::Hash(client.info().best_hash);
	let last_timestamp = client
		.runtime_api()
		.get_last_timestamp(&block_id)
		.expect("Get last timestamp");
	let timestamp = last_timestamp + cumulus_test_runtime::MinimumPeriod::get();

	inherent_data
		.put_data(sp_timestamp::INHERENT_IDENTIFIER, &timestamp)
		.expect("Put timestamp failed");
	inherent_data
		.put_data(
			VALIDATION_DATA_IDENTIFIER,
			&ValidationDataType {
				validation_data: validation_data.unwrap_or_default(),
				relay_chain_state: sp_state_machine::StorageProof::empty(),
			},
		)
		.expect("Put validation function params failed");

	client
		.runtime_api()
		.inherent_extrinsics_with_context(
			&BlockId::number(0),
			ExecutionContext::BlockConstruction,
			inherent_data,
		)
		.expect("Get inherents failed")
}
