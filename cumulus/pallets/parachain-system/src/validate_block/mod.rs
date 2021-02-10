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
pub mod implementation;
#[cfg(test)]
mod tests;

#[cfg(not(feature = "std"))]
#[doc(hidden)]
pub use polkadot_parachain;

/// Register the `validate_block` function that is used by parachains to validate blocks on a
/// validator.
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
///     cumulus_pallet_parachain_system::register_validate_block!(Block, BlockExecutor);
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
			unsafe fn validate_block(arguments: *const u8, arguments_len: usize) -> u64 {
				let params =
					$crate::validate_block::polkadot_parachain::load_params(arguments, arguments_len);

				let res = $crate::validate_block::implementation::validate_block::<
					$block,
					$block_executor,
				>(params);

				$crate::validate_block::polkadot_parachain::write_result(&res)
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
