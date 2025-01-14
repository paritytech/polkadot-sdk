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

//! Fees related instructions.

use codec::{Decode, Encode};
use scale_info::TypeInfo;

use crate::v6::{Asset, Location, WeightLimit};

/// Pay for the execution of some XCM `xcm` and `orders` with up to `weight`
/// picoseconds of execution time, paying for this with up to `fees` from the Holding Register.
///
/// - `fees`: The asset(s) to remove from the Holding Register to pay for fees.
/// - `weight_limit`: The maximum amount of weight to purchase; this must be at least the
///   expected maximum weight of the total XCM to be executed for the
///   `AllowTopLevelPaidExecutionFrom` barrier to allow the XCM be executed.
///
/// Kind: *Command*
///
/// Errors:
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct BuyExecution {
	pub fees: Asset,
	pub weight_limit: WeightLimit,
}

/// Refund any surplus weight previously bought with `BuyExecution`.
///
/// Kind: *Command*
///
/// Errors: None.
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct RefundSurplus;

/// A directive to indicate that the origin expects free execution of the message.
///
/// At execution time, this instruction just does a check on the Origin register.
/// However, at the barrier stage messages starting with this instruction can be disregarded if
/// the origin is not acceptable for free execution or the `weight_limit` is `Limited` and
/// insufficient.
///
/// Kind: *Indication*
///
/// Errors: If the given origin is `Some` and not equal to the current Origin register.
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct UnpaidExecution {
	pub weight_limit: WeightLimit,
	pub check_origin: Option<Location>,
}

/// Pay Fees.
///
/// Successor to `BuyExecution`.
/// Defined in fellowship RFC 105.
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct PayFees {
	pub asset: Asset,
}
