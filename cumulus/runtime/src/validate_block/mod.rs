// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <http://www.gnu.org/licenses/>.

//! A module that enables a runtime to work as parachain.

#[cfg(not(feature = "std"))]
#[doc(hidden)]
pub use rstd::slice;

#[cfg(not(feature = "std"))]
#[doc(hidden)]
pub mod storage_functions;
#[cfg(test)]
mod tests;

/// Register the `validate_block` function that is used by parachains to validate blocks on a validator.
///
/// Does *nothing* when `std` feature is enabled.
///
/// Expects as parameters the block and the block executor.
///
/// # Example
///
/// ```
///     struct Block;
///     struct BlockExecutor;
///
///     srml_parachain::register_validate_block!(Block, BlockExecutor);
///
/// # fn main() {}
/// ```
#[macro_export]
macro_rules! register_validate_block {
	($block:ty, $block_executor:ty) => {
		$crate::register_validate_block_impl!($block, $block_executor);
	};
}

/// The actual implementation of `register_validate_block` for `no_std`.
#[cfg(not(feature = "std"))]
#[doc(hidden)]
#[macro_export]
macro_rules! register_validate_block_impl {
	($block:ty, $block_executor:ty) => {
		#[doc(hidden)]
		mod parachain_validate_block {
			use super::*;

			#[no_mangle]
			unsafe fn validate_block(block: *const u8, block_len: u64, prev_head: *const u8, prev_head_len: u64) {
				let block = $crate::slice::from_raw_parts(block, block_len as usize);
				let prev_head = $crate::slice::from_raw_parts(prev_head, prev_head_len as usize);

				$crate::validate_block::validate_block::<$block, $block_executor>(block, prev_head);
			}
		}
	};
}

/// The actual implementation of `register_validate_block` for `std`.
#[cfg(feature = "std")]
#[doc(hidden)]
#[macro_export]
macro_rules! register_validate_block_impl {
	($block:ty, $block_executor:ty) => {};
}

/// Validate a given parachain block on a validator.
#[cfg(not(feature = "std"))]
#[doc(hidden)]
pub fn validate_block<Block: BlockT, E: ExecuteBlock<Block>>(mut block: &[u8], mut prev_head: &[u8]) {
	use codec::Decode;

	let block = ParachainBlock::<Block>::decode(&mut block).expect("Could not decode parachain block.");
	let parent_header = <<Block as BlockT>::Header as Decode>::decode(&mut prev_head).expect("Could not decode parent header.");

	let _guard = unsafe {
		use storage_functions as storage;
		STORAGE = Some(block.witness_data);
		(
			// Replace storage calls with our own implementations
			rio::ext_get_allocated_storage.replace_implementation(storage::ext_get_allocated_storage),
			rio::ext_get_storage_into.replace_implementation(storage::ext_get_storage_into),
			rio::ext_set_storage.replace_implementation(storage::ext_set_storage),
			rio::ext_exists_storage.replace_implementation(storage::ext_exists_storage),
			rio::ext_clear_storage.replace_implementation(storage::ext_clear_storage),
		)
	};

	let block_number = *parent_header.number() + One::one();
	E::execute_extrinsics_without_checks(block_number, block.extrinsics);
}