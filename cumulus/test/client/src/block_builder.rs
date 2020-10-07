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
use cumulus_primitives::{
	inherents::VALIDATION_FUNCTION_PARAMS_IDENTIFIER,
	validation_function_params::ValidationFunctionParams,
};
use runtime::GetLastTimestamp;
use sc_block_builder::BlockBuilderApi;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_core::ExecutionContext;
use sp_runtime::generic::BlockId;

/// Generate the inherents to a block so you don't have to.
pub fn generate_block_inherents(client: &Client) -> Vec<runtime::UncheckedExtrinsic> {
	let mut inherent_data = sp_consensus::InherentData::new();
	let block_id = BlockId::Hash(client.info().best_hash);
	let last_timestamp = client
		.runtime_api()
		.get_last_timestamp(&block_id)
		.expect("Get last timestamp");
	let timestamp = last_timestamp + runtime::MinimumPeriod::get();

	inherent_data
		.put_data(sp_timestamp::INHERENT_IDENTIFIER, &timestamp)
		.expect("Put timestamp failed");
	inherent_data
		.put_data(
			VALIDATION_FUNCTION_PARAMS_IDENTIFIER,
			&ValidationFunctionParams::default(),
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
