// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Shared helpers for the crate's test modules.

use crate::{
	alloy::hex,
	mock::{RuntimeEvent, System, Test},
	pallet,
	IERC20::IERC20Events,
};
use pallet_revive::precompiles::alloy::{self, primitives::IntoLogData};
use sp_core::{H160, H256};

alloy::sol! {
	/// Solidity interface for the `Caller` fixture contract. Shared between
	/// `tests.rs` and `permit_tests.rs` so the two suites drive STATICCALL /
	/// DELEGATECALL through one canonical declaration.
	interface ICaller {
		function staticCall(address callee, bytes data, uint64 gas) external view returns (bool success, bytes output);
		function delegate(address callee, bytes data, uint64 gas) external returns (bool success, bytes output);
	}
}

pub(crate) const PRECOMPILE_ADDRESS_PREFIX: u16 = 0x0120;
pub(crate) const PRECOMPILE_ADDRESS_PREFIX_FOREIGN: u16 = 0x0220;

pub(crate) fn set_prefix_in_address(prefix: u16) -> [u8; 20] {
	let mut addr = hex::const_decode_to_array(b"0000000000000000000000000000000000000000").unwrap();
	addr[16..18].copy_from_slice(&prefix.to_be_bytes());
	addr
}

/// The token (precompile) address for `index` under `prefix`: the prefix address with the index
/// inlined big-endian into the first four bytes.
pub(crate) fn token_address(prefix: u16, index: u32) -> H160 {
	let mut addr = set_prefix_in_address(prefix);
	addr[..4].copy_from_slice(&index.to_be_bytes());
	H160::from(addr)
}

/// Assert `event` was emitted from `contract` exactly once — duplicates (e.g. a precompile
/// log next to the `Erc20TransferLogsCallback`-mirrored one) are a failure, not a pass.
pub(crate) fn assert_contract_event(contract: H160, event: IERC20Events) {
	let (topics, data) = event.into_log_data().split();
	let topics = topics.into_iter().map(|v| H256(v.0)).collect::<Vec<_>>();
	let expected = RuntimeEvent::Revive(pallet_revive::Event::ContractEmitted {
		contract,
		data: data.to_vec(),
		topics,
	});
	let count = System::events().iter().filter(|record| record.event == expected).count();
	assert_eq!(count, 1, "expected exactly one occurrence of {expected:?}, got {count}");
}

pub(crate) fn setup_asset_for_prefix(asset_id: u32, prefix: u16) {
	if prefix == PRECOMPILE_ADDRESS_PREFIX_FOREIGN {
		pallet::Pallet::<Test>::insert_asset_mapping(&asset_id)
			.expect("Failed to insert asset mapping");
	}
}
