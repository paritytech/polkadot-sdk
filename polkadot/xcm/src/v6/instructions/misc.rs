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

//! Miscellaneous instructions.

use codec::{Decode, Encode};
use scale_info::TypeInfo;
use educe::Educe;

use crate::v6::{InteriorLocation, NetworkId, OriginKind, Weight, Xcm};
use crate::DoubleEncoded;

/// Apply the encoded transaction `call`, whose dispatch-origin should be `origin` as expressed
/// by the kind of origin `origin_kind`.
///
/// The Transact Status Register is set according to the result of dispatching the call.
///
/// - `origin_kind`: The means of expressing the message origin as a dispatch origin.
/// - `call`: The encoded transaction to be applied.
/// - `fallback_max_weight`: Used for compatibility with previous versions. Corresponds to the
///   `require_weight_at_most` parameter in previous versions. If you don't care about
///   compatibility you can just put `None`. WARNING: If you do, your XCM might not work with
///   older versions. Make sure to dry-run and validate.
///
/// Safety: No concerns.
///
/// Kind: *Command*.
///
/// Errors:
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone(bound = false), Eq, PartialEq(bound = false), Debug(bound = false))]
#[scale_info(skip_type_params(Call))]
pub struct Transact<Call> {
	pub origin_kind: OriginKind,
	pub fallback_max_weight: Option<Weight>,
	pub call: DoubleEncoded<Call>,
}

impl<Call> Transact<Call> {
	pub fn into<C>(self) -> Transact<C> {
		Transact::from(self)
	}

	pub fn from<C>(xcm: Transact<C>) -> Self {
		Self {
			origin_kind: xcm.origin_kind,
			fallback_max_weight: xcm.fallback_max_weight,
			call: xcm.call.into(),
		}
	}
}

/// Send a message on to Non-Local Consensus system.
///
/// This will tend to utilize some extra-consensus mechanism, the obvious one being a bridge.
/// A fee may be charged; this may be determined based on the contents of `xcm`. It will be
/// taken from the Holding register.
///
/// - `network`: The remote consensus system to which the message should be exported.
/// - `destination`: The location relative to the remote consensus system to which the message
///   should be sent on arrival.
/// - `xcm`: The message to be exported.
///
/// As an example, to export a message for execution on Statemine (parachain #1000 in the
/// Kusama network), you would call with `network: NetworkId::Kusama` and
/// `destination: [Parachain(1000)].into()`. Alternatively, to export a message for execution
/// on Polkadot, you would call with `network: NetworkId:: Polkadot` and `destination: Here`.
///
/// Kind: *Command*
///
/// Errors: *Fallible*.
#[derive(Clone, Eq, PartialEq, Debug, Encode, Decode, TypeInfo)]
pub struct ExportMessage {
	pub network: NetworkId,
	pub destination: InteriorLocation,
	pub xcm: Xcm<()>,
}
