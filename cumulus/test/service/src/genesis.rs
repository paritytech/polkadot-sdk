// Copyright (C) Parity Technologies (UK) Ltd.
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

use codec::Encode;
use cumulus_client_cli::generate_genesis_block;
use cumulus_primitives_core::ParaId;
use cumulus_test_runtime::Block;
use polkadot_primitives::HeadData;
use sp_runtime::traits::Block as BlockT;

/// Returns the initial head data for a parachain ID.
pub fn initial_head_data(para_id: ParaId) -> HeadData {
	let spec = crate::chain_spec::get_chain_spec(para_id);
	let block: Block = generate_genesis_block(&spec, sp_runtime::StateVersion::V1).unwrap();
	let genesis_state = block.header().encode();
	genesis_state.into()
}
