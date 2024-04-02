// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Weight-related utilities.

use crate::weights::WeightInfo;

use frame_support::pallet_prelude::Weight;

/// Extended weight info.
pub trait WeightInfoExt: WeightInfo {
	/// Returns weight, that needs to be added to the pre-dispatch weight of message delivery call,
	/// if `RefundBridgedParachainMessages` signed extension is deployed at runtime level.
	fn receive_messages_proof_overhead_from_runtime() -> Weight {
		Self::slash_and_deregister().max(Self::register_relayer_reward())
	}

	/// Returns weight, that needs to be added to the pre-dispatch weight of message delivery
	/// confirmation call, if `RefundBridgedParachainMessages` signed extension is deployed at
	/// runtime level.
	fn receive_messages_delivery_proof_overhead_from_runtime() -> Weight {
		Self::register_relayer_reward()
	}

	/// Returns weight that we need to deduct from the message delivery call weight that has
	/// completed successfully.
	///
	/// Usually, the weight of `slash_and_deregister` is larger than the weight of the
	/// `register_relayer_reward`. So if relayer has been rewarded, we want to deduct the difference
	/// to get the actual post-dispatch weight.
	fn extra_weight_of_successful_receive_messages_proof_call() -> Weight {
		Self::slash_and_deregister().saturating_sub(Self::register_relayer_reward())
	}
}

impl<T: WeightInfo> WeightInfoExt for T {}
