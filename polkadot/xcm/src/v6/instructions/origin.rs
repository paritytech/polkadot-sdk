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

//! Origin related instructions.

/// Clear the origin.
///
/// This may be used by the XCM author to ensure that later instructions cannot command the
/// authority of the origin (e.g. if they are being relayed from an untrusted source, as often
/// the case with `ReserveAssetDeposited`).
///
/// Safety: No concerns.
///
/// Kind: *Command*.
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct ClearOrigin;

/// Mutate the origin to some interior location.
///
/// Kind: *Command*
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct DescendOrigin(pub InteriorLocation);

/// Set the Origin Register to be some child of the Universal Ancestor.
///
/// Safety: Should only be usable if the Origin is trusted to represent the Universal Ancestor
/// child in general. In general, no Origin should be able to represent the Universal Ancestor
/// child which is the root of the local consensus system since it would by extension
/// allow it to act as any location within the local consensus.
///
/// The `Junction` parameter should generally be a `GlobalConsensus` variant since it is only
/// these which are children of the Universal Ancestor.
///
/// Kind: *Command*
///
/// Errors: *Fallible*.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct UniversalOrigin(pub Junction);

/// Alter the current Origin to another given origin.
///
/// Kind: *Command*
///
/// Errors: If the existing state would not allow such a change.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct AliasOrigin(pub Location);

/// Executes inner `xcm` with origin set to the provided `descendant_origin`. Once the inner
/// `xcm` is executed, the original origin (the one active for this instruction) is restored.
///
/// Parameters:
/// - `descendant_origin`: The origin that will be used during the execution of the inner
///   `xcm`. If set to `None`, the inner `xcm` is executed with no origin. If set to `Some(o)`,
///   the inner `xcm` is executed as if there was a `DescendOrigin(o)` executed before it, and
///   runs the inner xcm with origin: `original_origin.append_with(o)`.
/// - `xcm`: Inner instructions that will be executed with the origin modified according to
///   `descendant_origin`.
///
/// Safety: No concerns.
///
/// Kind: *Command*
///
/// Errors:
/// - `BadOrigin`
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
#[scale_info(skip_type_params(Call))]
pub struct ExecuteWithOrigin<Call> {
	// TODO: make this generic over Xcm so it is using the current version
	pub descendant_origin: Option<InteriorLocation>,
	pub xcm: Xcm<Call>,
}

impl<Call> ExecuteWithOrigin<Call> {
	pub fn into<C>(self) -> ExecuteWithOrigin<C> {
		ExecuteWithOrigin::from(self)
	}

	pub fn from<C>(xcm: ExecuteWithOrigin<C>) -> Self {
		Self { descendant_origin: xcm.descendant_origin, xcm: xcm.xcm.into() }
	}
}
