// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

use polkadot_parachain::primitives::ValidationParams;

#[cfg(not(feature = "std"))]
#[doc(hidden)]
pub mod implementation;
#[cfg(test)]
mod tests;

#[cfg(not(feature = "std"))]
#[doc(hidden)]
pub use polkadot_parachain;
#[cfg(not(feature = "std"))]
#[doc(hidden)]
pub use sp_runtime::traits::GetRuntimeBlockType;

// Stores the [`ValidationParams`] that are being passed to `validate_block`.
//
// This value will only be set when a parachain validator validates a given `PoV`.
environmental::environmental!(VALIDATION_PARAMS: ValidationParams);

/// Execute the given closure with the [`ValidationParams`].
///
/// Returns `None` if the [`ValidationParams`] are not set, because the code is currently not being
/// executed in the context of `validate_block`.
pub(crate) fn with_validation_params<R>(f: impl FnOnce(&ValidationParams) -> R) -> Option<R> {
	VALIDATION_PARAMS::with(|v| f(v))
}

/// Set the [`ValidationParams`] for the local context and execute the given closure in this context.
#[cfg(not(feature = "std"))]
fn set_and_run_with_validation_params<R>(mut params: ValidationParams, f: impl FnOnce() -> R) -> R {
	VALIDATION_PARAMS::using(&mut params, f)
}

/// Register the `validate_block` function that is used by parachains to validate blocks on a
/// validator.
///
/// Does *nothing* when `std` feature is enabled.
///
/// Expects as parameters the runtime and a block executor.
///
/// # Example
///
/// ```
///     struct BlockExecutor;
///     struct Runtime;
///
///     cumulus_pallet_parachain_system::register_validate_block!(Runtime, BlockExecutor);
///
/// # fn main() {}
/// ```
#[macro_export]
macro_rules! register_validate_block {
	($runtime:ty, $block_executor:ty) => {
		$crate::register_validate_block_impl!($runtime, $block_executor);
	};
}

/// The actual implementation of `register_validate_block` for `no_std`.
#[cfg(not(feature = "std"))]
#[doc(hidden)]
#[macro_export]
macro_rules! register_validate_block_impl {
	($runtime:ty, $block_executor:ty) => {
		#[doc(hidden)]
		mod parachain_validate_block {
			use super::*;

			#[no_mangle]
			unsafe fn validate_block(arguments: *const u8, arguments_len: usize) -> u64 {
				let params = $crate::validate_block::polkadot_parachain::load_params(
					arguments,
					arguments_len,
				);

				let res = $crate::validate_block::implementation::validate_block::<
					<$runtime as $crate::validate_block::GetRuntimeBlockType>::RuntimeBlock,
					$block_executor,
					$runtime,
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
	($runtime:ty, $block_executor:ty) => {};
}
