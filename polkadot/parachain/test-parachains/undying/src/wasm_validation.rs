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

//! WASM validation for the `Undying` parachain.

use crate::{BlockData, HeadData};
#[cfg(rfc145)]
use alloc::vec;
use codec::{Decode, Encode};
use polkadot_parachain_primitives::primitives::{
	HeadData as GenericHeadData, ValidationParams, ValidationResult,
};

// RFC-145 (V2) entry point: the input data is pulled in through the `input::read` host
// function instead of being written into the runtime memory by the host.
#[cfg(rfc145)]
#[no_mangle]
pub extern "C" fn validate_block(arguments_len: usize) -> u64 {
	let mut buf = vec![0u8; arguments_len];
	sp_io::input::read(&mut buf[..]);
	let params = ValidationParams::decode(&mut &buf[..]).expect("Invalid input data");
	do_validate_block(params)
}

// Legacy (V1) entry point: the host allocates runtime memory and writes the input data into
// it before the call.
#[cfg(not(rfc145))]
#[no_mangle]
pub extern "C" fn validate_block(params: *const u8, len: usize) -> u64 {
	let params = unsafe { polkadot_parachain_primitives::load_params(params, len) };
	do_validate_block(params)
}

fn do_validate_block(params: ValidationParams) -> u64 {
	let parent_head =
		HeadData::decode(&mut &params.parent_head.0[..]).expect("invalid parent head format.");

	let mut block_data =
		BlockData::decode(&mut &params.block_data.0[..]).expect("invalid block data format.");

	let parent_hash = crate::keccak256(&params.parent_head.0[..]);

	let (new_head, _, upward_messages) =
		crate::execute(parent_hash, parent_head, block_data).expect("Executes block");

	polkadot_parachain_primitives::write_result(&ValidationResult {
		head_data: GenericHeadData(new_head.encode()),
		new_validation_code: None,
		upward_messages,
		horizontal_messages: alloc::vec::Vec::new()
			.try_into()
			.expect("empty vec fits within bounds"),
		processed_downward_messages: 0,
		hrmp_watermark: params.relay_parent_number,
	})
}
