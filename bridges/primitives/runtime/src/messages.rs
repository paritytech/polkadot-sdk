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

//! Primitives that may be used by different message delivery and dispatch mechanisms.

use codec::{Decode, Encode};
use frame_support::weights::Weight;
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;

/// Message dispatch result.
#[derive(Encode, Decode, RuntimeDebug, Clone, PartialEq, Eq, TypeInfo)]
pub struct MessageDispatchResult<DispatchLevelResult> {
	/// Unspent dispatch weight. This weight that will be deducted from total delivery transaction
	/// weight, thus reducing the transaction cost. This shall not be zero in (at least) two cases:
	///
	/// 1) if message has been dispatched successfully, but post-dispatch weight is less than the
	///    weight, declared by the message sender;
	/// 2) if message has not been dispatched at all.
	pub unspent_weight: Weight,
	/// Fine-grained result of single message dispatch (for better diagnostic purposes)
	pub dispatch_level_result: DispatchLevelResult,
}
