// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use polkadot_node_subsystem_test_helpers::mock::new_block_import_info;
use polkadot_overseer::BlockInfo;
use polkadot_primitives::{BlockNumber, Hash};

use crate::configuration::{TestAuthorities, TestConfiguration};

pub struct TestState {
	// Full test config
	pub config: TestConfiguration,
	// Authority keys for the network emulation.
	pub test_authorities: TestAuthorities,
	// Relay chain block infos
	pub block_infos: Vec<BlockInfo>,
}

impl TestState {
	pub fn new(config: &TestConfiguration) -> Self {
		Self {
			config: config.clone(),
			test_authorities: config.generate_authorities(),
			block_infos: (1..=config.num_blocks)
				.map(|block_num| {
					let relay_block_hash = Hash::repeat_byte(block_num as u8);
					new_block_import_info(relay_block_hash, block_num as BlockNumber)
				})
				.collect(),
		}
	}
}
