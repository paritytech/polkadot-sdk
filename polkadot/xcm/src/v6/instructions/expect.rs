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

//! Expect related instructions.

/// Throw an error if Holding does not contain at least the given assets.
///
/// Kind: *Command*
///
/// Errors:
/// - `ExpectationFalse`: If Holding Register does not contain the assets in the parameter.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct ExpectAsset(pub Assets);

/// Ensure that the Origin Register equals some given value and throw an error if not.
///
/// Kind: *Command*
///
/// Errors:
/// - `ExpectationFalse`: If Origin Register is not equal to the parameter.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct ExpectOrigin(pub Option<Location>);

/// Ensure that the Error Register equals some given value and throw an error if not.
///
/// Kind: *Command*
///
/// Errors:
/// - `ExpectationFalse`: If the value of the Error Register is not equal to the parameter.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct ExpectError(pub Option<(u32, Error)>);

/// Ensure that the Transact Status Register equals some given value and throw an error if
/// not.
///
/// Kind: *Command*
///
/// Errors:
/// - `ExpectationFalse`: If the value of the Transact Status Register is not equal to the
///   parameter.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct ExpectTransactStatus(pub MaybeErrorCode);

/// Ensure that a particular pallet with a particular version exists.
///
/// - `index: Compact`: The index which identifies the pallet. An error if no pallet exists at
///   this index.
/// - `name: Vec<u8>`: Name which must be equal to the name of the pallet.
/// - `module_name: Vec<u8>`: Module name which must be equal to the name of the module in
///   which the pallet exists.
/// - `crate_major: Compact`: Version number which must be equal to the major version of the
///   crate which implements the pallet.
/// - `min_crate_minor: Compact`: Version number which must be at most the minor version of the
///   crate which implements the pallet.
///
/// Safety: No concerns.
///
/// Kind: *Command*
///
/// Errors:
/// - `ExpectationFalse`: In case any of the expectations are broken.
#[derive(Educe, Encode, Decode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
pub struct ExpectPallet {
	#[codec(compact)]
	pub index: u32,
	pub name: Vec<u8>,
	pub module_name: Vec<u8>,
	#[codec(compact)]
	pub crate_major: u32,
	#[codec(compact)]
	pub min_crate_minor: u32,
}
