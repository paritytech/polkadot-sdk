// Copyright 2019-2020 Parity Technologies (UK) Ltd.
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
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Runtime API definition for transaction payment module.

#![cfg_attr(not(feature = "std"), no_std)]

use sp_std::prelude::*;
use frame_support::weights::{Weight, DispatchClass};
use codec::{Encode, Codec, Decode};
#[cfg(feature = "std")]
use serde::{Serialize, Deserialize};
use sp_runtime::traits::{UniqueSaturatedInto, SaturatedConversion};

/// Some information related to a dispatchable that can be queried from the runtime.
#[derive(Eq, PartialEq, Encode, Decode, Default)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct RuntimeDispatchInfo<Balance> {
	/// Weight of this dispatch.
	pub weight: Weight,
	/// Class of this dispatch.
	pub class: DispatchClass,
	/// The partial inclusion fee of this dispatch. This does not include tip or anything else which
	/// is dependent on the signature (aka. depends on a `SignedExtension`).
	pub partial_fee: Balance,
}

/// A capped version of `RuntimeDispatchInfo`.
///
/// The `Balance` is capped (or expanded) to `u64` to avoid serde issues with `u128`.
#[derive(Eq, PartialEq, Encode, Decode, Default)]
#[cfg_attr(feature = "std", derive(Debug, Serialize, Deserialize))]
#[cfg_attr(feature = "std", serde(rename_all = "camelCase"))]
pub struct CappedDispatchInfo {
	/// Weight of this dispatch.
	pub weight: Weight,
	/// Class of this dispatch.
	pub class: DispatchClass,
	/// The partial inclusion fee of this dispatch. This does not include tip or anything else which
	/// is dependent on the signature (aka. depends on a `SignedExtension`).
	pub partial_fee: u64,
}

impl CappedDispatchInfo {
	/// Create a new `CappedDispatchInfo` from `RuntimeDispatchInfo`.
	pub fn new<Balance: UniqueSaturatedInto<u64>>(
		dispatch: RuntimeDispatchInfo<Balance>,
	) -> Self {
		let RuntimeDispatchInfo {
			weight,
			class,
			partial_fee,
		} = dispatch;

		Self {
			weight,
			class,
			partial_fee: partial_fee.saturated_into(),
		}
	}
}

sp_api::decl_runtime_apis! {
	pub trait TransactionPaymentApi<Balance, Extrinsic> where
		Balance: Codec,
		Extrinsic: Codec,
	{
		fn query_info(uxt: Extrinsic, len: u32) -> RuntimeDispatchInfo<Balance>;
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn should_serialize_properly_with_u64() {
		let info = RuntimeDispatchInfo {
			weight: 5,
			class: DispatchClass::Normal,
			partial_fee: 1_000_000_u64,
		};

		let info = CappedDispatchInfo::new(info);
		assert_eq!(
			serde_json::to_string(&info).unwrap(),
			r#"{"weight":5,"class":"normal","partialFee":1000000}"#,
		);

		// should not panic
		serde_json::to_value(&info).unwrap();
	}
}
