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

//! Cross-Consensus Message format data structures.

// NOTE, this crate is meant to be used in many different environments, notably wasm, but not
// necessarily related to FRAME or even Substrate.
//
// Hence, `no_std` rather than sp-runtime.
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use codec::{Decode, DecodeLimit, Encode, Error as CodecError, Input, MaxEncodedLen};
use derivative::Derivative;
use scale_info::TypeInfo;

pub mod v3;
pub mod v4;
pub mod v5;

pub mod lts {
	pub use super::v4::*;
}

pub mod latest {
	pub use super::v5::*;
}

mod double_encoded;
pub use double_encoded::DoubleEncoded;

#[cfg(test)]
mod tests;

/// Maximum nesting level for XCM decoding.
pub const MAX_XCM_DECODE_DEPTH: u32 = 8;

/// A version of XCM.
pub type Version = u32;

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum Unsupported {}
impl Encode for Unsupported {}
impl Decode for Unsupported {
	fn decode<I: Input>(_: &mut I) -> Result<Self, CodecError> {
		Err("Not decodable".into())
	}
}

/// Attempt to convert `self` into a particular version of itself.
pub trait IntoVersion: Sized {
	/// Consume `self` and return same value expressed in some particular `version` of XCM.
	fn into_version(self, version: Version) -> Result<Self, ()>;

	/// Consume `self` and return same value expressed the latest version of XCM.
	fn into_latest(self) -> Result<Self, ()> {
		self.into_version(latest::VERSION)
	}
}

pub trait TryAs<T> {
	fn try_as(&self) -> Result<&T, ()>;
}

// Macro that generated versioned wrapper types.
// NOTE: converting a v4 type into a versioned type will make it v5.
macro_rules! versioned_type {
	($(#[$attr:meta])* pub enum $n:ident {
		$(#[$index3:meta])+
		V3($v3:ty),
		$(#[$index4:meta])+
		V4($v4:ty),
		$(#[$index5:meta])+
		V5($v5:ty),
	}) => {
		#[derive(Derivative, Encode, Decode, TypeInfo)]
		#[derivative(
			Clone(bound = ""),
			Eq(bound = ""),
			PartialEq(bound = ""),
			Debug(bound = "")
		)]
		#[codec(encode_bound())]
		#[codec(decode_bound())]
		#[scale_info(replace_segment("staging_xcm", "xcm"))]
		$(#[$attr])*
		pub enum $n {
			$(#[$index3])*
			V3($v3),
			$(#[$index4])*
			V4($v4),
			$(#[$index5])*
			V5($v5),
		}
		impl $n {
			pub fn try_as<T>(&self) -> Result<&T, ()> where Self: TryAs<T> {
				<Self as TryAs<T>>::try_as(&self)
			}
		}
		impl TryAs<$v3> for $n {
			fn try_as(&self) -> Result<&$v3, ()> {
				match &self {
					Self::V3(ref x) => Ok(x),
					_ => Err(()),
				}
			}
		}
		impl TryAs<$v4> for $n {
			fn try_as(&self) -> Result<&$v4, ()> {
				match &self {
					Self::V4(ref x) => Ok(x),
					_ => Err(()),
				}
			}
		}
		impl TryAs<$v5> for $n {
			fn try_as(&self) -> Result<&$v5, ()> {
				match &self {
					Self::V5(ref x) => Ok(x),
					_ => Err(()),
				}
			}
		}
		impl IntoVersion for $n {
			fn into_version(self, n: Version) -> Result<Self, ()> {
				Ok(match n {
					3 => Self::V3(self.try_into()?),
					4 => Self::V4(self.try_into()?),
					5 => Self::V5(self.try_into()?),
					_ => return Err(()),
				})
			}
		}
		impl From<$v3> for $n {
			fn from(x: $v3) -> Self {
				$n::V3(x.into())
			}
		}
		impl<T: Into<$v5>> From<T> for $n {
			fn from(x: T) -> Self {
				$n::V5(x.into())
			}
		}
		impl TryFrom<$n> for $v3 {
			type Error = ();
			fn try_from(x: $n) -> Result<Self, ()> {
				use $n::*;
				match x {
					V3(x) => Ok(x),
					V4(x) => x.try_into().map_err(|_| ()),
					V5(x) => {
						let v4: $v4 = x.try_into().map_err(|_| ())?;
						v4.try_into().map_err(|_| ())
					}
				}
			}
		}
		impl TryFrom<$n> for $v4 {
			type Error = ();
			fn try_from(x: $n) -> Result<Self, ()> {
				use $n::*;
				match x {
					V3(x) => x.try_into().map_err(|_| ()),
					V4(x) => Ok(x),
					V5(x) => x.try_into().map_err(|_| ()),
				}
			}
		}
		impl TryFrom<$n> for $v5 {
			type Error = ();
			fn try_from(x: $n) -> Result<Self, ()> {
				use $n::*;
				match x {
					V3(x) => {
						let v4: $v4 = x.try_into().map_err(|_| ())?;
						v4.try_into().map_err(|_| ())
					},
					V4(x) => x.try_into().map_err(|_| ()),
					V5(x) => Ok(x),
				}
			}
		}
		impl MaxEncodedLen for $n {
			fn max_encoded_len() -> usize {
				<$v3>::max_encoded_len()
			}
		}
		impl IdentifyVersion for $n {
			fn identify_version(&self) -> Version {
				use $n::*;
				match self {
					V3(_) => v3::VERSION,
					V4(_) => v4::VERSION,
					V5(_) => v5::VERSION,
				}
			}
		}
	};
}

versioned_type! {
	/// A single version's `AssetId` value, together with its version code.
	pub enum VersionedAssetId {
		#[codec(index = 3)]
		V3(v3::AssetId),
		#[codec(index = 4)]
		V4(v4::AssetId),
		#[codec(index = 5)]
		V5(v5::AssetId),
	}
}

versioned_type! {
	/// A single version's `Response` value, together with its version code.
	pub enum VersionedResponse {
		#[codec(index = 3)]
		V3(v3::Response),
		#[codec(index = 4)]
		V4(v4::Response),
		#[codec(index = 5)]
		V5(v5::Response),
	}
}

versioned_type! {
	/// A single `NetworkId` value, together with its version code.
	pub enum VersionedNetworkId {
		#[codec(index = 3)]
		V3(v3::NetworkId),
		#[codec(index = 4)]
		V4(v4::NetworkId),
		#[codec(index = 5)]
		V5(v5::NetworkId),
	}
}

versioned_type! {
	/// A single `Junction` value, together with its version code.
	pub enum VersionedJunction {
		#[codec(index = 3)]
		V3(v3::Junction),
		#[codec(index = 4)]
		V4(v4::Junction),
		#[codec(index = 5)]
		V5(v5::Junction),
	}
}

versioned_type! {
	/// A single `Location` value, together with its version code.
	#[derive(Ord, PartialOrd)]
	pub enum VersionedLocation {
		#[codec(index = 3)]
		V3(v3::MultiLocation),
		#[codec(index = 4)]
		V4(v4::Location),
		#[codec(index = 5)]
		V5(v5::Location),
	}
}

versioned_type! {
	/// A single `InteriorLocation` value, together with its version code.
	pub enum VersionedInteriorLocation {
		#[codec(index = 3)]
		V3(v3::InteriorMultiLocation),
		#[codec(index = 4)]
		V4(v4::InteriorLocation),
		#[codec(index = 5)]
		V5(v5::InteriorLocation),
	}
}

versioned_type! {
	/// A single `Asset` value, together with its version code.
	pub enum VersionedAsset {
		#[codec(index = 3)]
		V3(v3::MultiAsset),
		#[codec(index = 4)]
		V4(v4::Asset),
		#[codec(index = 5)]
		V5(v5::Asset),
	}
}

versioned_type! {
	/// A single `MultiAssets` value, together with its version code.
	pub enum VersionedAssets {
		#[codec(index = 3)]
		V3(v3::MultiAssets),
		#[codec(index = 4)]
		V4(v4::Assets),
		#[codec(index = 5)]
		V5(v5::Assets),
	}
}

/// A single XCM message, together with its version code.
#[derive(Derivative, Encode, Decode, TypeInfo)]
#[derivative(Clone(bound = ""), Eq(bound = ""), PartialEq(bound = ""), Debug(bound = ""))]
#[codec(encode_bound())]
#[codec(decode_bound())]
#[scale_info(bounds(), skip_type_params(RuntimeCall))]
#[scale_info(replace_segment("staging_xcm", "xcm"))]
pub enum VersionedXcm<RuntimeCall> {
	#[codec(index = 3)]
	V3(v3::Xcm<RuntimeCall>),
	#[codec(index = 4)]
	V4(v4::Xcm<RuntimeCall>),
	#[codec(index = 5)]
	V5(v5::Xcm<RuntimeCall>),
}

impl<C> IntoVersion for VersionedXcm<C> {
	fn into_version(self, n: Version) -> Result<Self, ()> {
		Ok(match n {
			3 => Self::V3(self.try_into()?),
			4 => Self::V4(self.try_into()?),
			5 => Self::V5(self.try_into()?),
			_ => return Err(()),
		})
	}
}

impl<C> IdentifyVersion for VersionedXcm<C> {
	fn identify_version(&self) -> Version {
		match self {
			Self::V3(_) => v3::VERSION,
			Self::V4(_) => v4::VERSION,
			Self::V5(_) => v5::VERSION,
		}
	}
}

impl<C> VersionedXcm<C> {
	/// Checks that the XCM is decodable with `MAX_XCM_DECODE_DEPTH`. Consequently, it also checks
	/// all decode implementations and limits, such as MAX_ITEMS_IN_ASSETS or
	/// MAX_INSTRUCTIONS_TO_DECODE.
	///
	/// Note that this uses the limit of the sender - not the receiver. It is a best effort.
	pub fn validate_xcm_nesting(&self) -> Result<(), ()> {
		self.using_encoded(|mut enc| {
			Self::decode_all_with_depth_limit(MAX_XCM_DECODE_DEPTH, &mut enc).map(|_| ())
		})
		.map_err(|e| {
			log::error!(target: "xcm::validate_xcm_nesting", "Decode error: {e:?} for xcm: {self:?}!");
			()
		})
	}
}

impl<RuntimeCall> From<v3::Xcm<RuntimeCall>> for VersionedXcm<RuntimeCall> {
	fn from(x: v3::Xcm<RuntimeCall>) -> Self {
		VersionedXcm::V3(x)
	}
}

impl<RuntimeCall> From<v4::Xcm<RuntimeCall>> for VersionedXcm<RuntimeCall> {
	fn from(x: v4::Xcm<RuntimeCall>) -> Self {
		VersionedXcm::V4(x)
	}
}

impl<RuntimeCall> From<v5::Xcm<RuntimeCall>> for VersionedXcm<RuntimeCall> {
	fn from(x: v5::Xcm<RuntimeCall>) -> Self {
		VersionedXcm::V5(x)
	}
}

impl<Call> TryFrom<VersionedXcm<Call>> for v3::Xcm<Call> {
	type Error = ();
	fn try_from(x: VersionedXcm<Call>) -> Result<Self, ()> {
		use VersionedXcm::*;
		match x {
			V3(x) => Ok(x),
			V4(x) => x.try_into(),
			V5(x) => {
				let v4: v4::Xcm<Call> = x.try_into()?;
				v4.try_into()
			},
		}
	}
}

impl<Call> TryFrom<VersionedXcm<Call>> for v4::Xcm<Call> {
	type Error = ();
	fn try_from(x: VersionedXcm<Call>) -> Result<Self, ()> {
		use VersionedXcm::*;
		match x {
			V3(x) => x.try_into(),
			V4(x) => Ok(x),
			V5(x) => x.try_into(),
		}
	}
}

impl<Call> TryFrom<VersionedXcm<Call>> for v5::Xcm<Call> {
	type Error = ();
	fn try_from(x: VersionedXcm<Call>) -> Result<Self, ()> {
		use VersionedXcm::*;
		match x {
			V3(x) => {
				let v4: v4::Xcm<Call> = x.try_into()?;
				v4.try_into()
			},
			V4(x) => x.try_into(),
			V5(x) => Ok(x),
		}
	}
}

/// Convert an `Xcm` datum into a `VersionedXcm`, based on a destination `Location` which will
/// interpret it.
pub trait WrapVersion {
	fn wrap_version<RuntimeCall>(
		dest: &latest::Location,
		xcm: impl Into<VersionedXcm<RuntimeCall>>,
	) -> Result<VersionedXcm<RuntimeCall>, ()>;
}

/// Used to get the version out of a versioned type.
// TODO(XCMv5): This could be `GetVersion` and we change the current one to `GetVersionFor`.
pub trait IdentifyVersion {
	fn identify_version(&self) -> Version;
}

/// Check and return the `Version` that should be used for the `Xcm` datum for the destination
/// `Location`, which will interpret it.
pub trait GetVersion {
	fn get_version_for(dest: &latest::Location) -> Option<Version>;
}

/// `()` implementation does nothing with the XCM, just sending with whatever version it was
/// authored as.
impl WrapVersion for () {
	fn wrap_version<RuntimeCall>(
		_: &latest::Location,
		xcm: impl Into<VersionedXcm<RuntimeCall>>,
	) -> Result<VersionedXcm<RuntimeCall>, ()> {
		Ok(xcm.into())
	}
}

/// `WrapVersion` implementation which attempts to always convert the XCM to version 3 before
/// wrapping it.
pub struct AlwaysV3;
impl WrapVersion for AlwaysV3 {
	fn wrap_version<Call>(
		_: &latest::Location,
		xcm: impl Into<VersionedXcm<Call>>,
	) -> Result<VersionedXcm<Call>, ()> {
		Ok(VersionedXcm::<Call>::V3(xcm.into().try_into()?))
	}
}
impl GetVersion for AlwaysV3 {
	fn get_version_for(_dest: &latest::Location) -> Option<Version> {
		Some(v3::VERSION)
	}
}

/// `WrapVersion` implementation which attempts to always convert the XCM to version 3 before
/// wrapping it.
pub struct AlwaysV4;
impl WrapVersion for AlwaysV4 {
	fn wrap_version<Call>(
		_: &latest::Location,
		xcm: impl Into<VersionedXcm<Call>>,
	) -> Result<VersionedXcm<Call>, ()> {
		Ok(VersionedXcm::<Call>::V4(xcm.into().try_into()?))
	}
}
impl GetVersion for AlwaysV4 {
	fn get_version_for(_dest: &latest::Location) -> Option<Version> {
		Some(v4::VERSION)
	}
}

/// `WrapVersion` implementation which attempts to always convert the XCM to version 3 before
/// wrapping it.
pub struct AlwaysV5;
impl WrapVersion for AlwaysV5 {
	fn wrap_version<Call>(
		_: &latest::Location,
		xcm: impl Into<VersionedXcm<Call>>,
	) -> Result<VersionedXcm<Call>, ()> {
		Ok(VersionedXcm::<Call>::V5(xcm.into().try_into()?))
	}
}
impl GetVersion for AlwaysV5 {
	fn get_version_for(_dest: &latest::Location) -> Option<Version> {
		Some(v5::VERSION)
	}
}

/// `WrapVersion` implementation which attempts to always convert the XCM to the latest version
/// before wrapping it.
pub type AlwaysLatest = AlwaysV5;

/// `WrapVersion` implementation which attempts to always convert the XCM to the most recent Long-
/// Term-Support version before wrapping it.
pub type AlwaysLts = AlwaysV4;

pub mod prelude {
	pub use super::{
		latest::prelude::*, AlwaysLatest, AlwaysLts, AlwaysV3, AlwaysV4, AlwaysV5, GetVersion,
		IdentifyVersion, IntoVersion, Unsupported, Version as XcmVersion, VersionedAsset,
		VersionedAssetId, VersionedAssets, VersionedInteriorLocation, VersionedLocation,
		VersionedResponse, VersionedXcm, WrapVersion,
	};
}

pub mod opaque {
	pub mod v3 {
		// Everything from v3
		pub use crate::v3::*;
		// Then override with the opaque types in v3
		pub use crate::v3::opaque::{Instruction, Xcm};
	}
	pub mod v4 {
		// Everything from v4
		pub use crate::v4::*;
		// Then override with the opaque types in v4
		pub use crate::v4::opaque::{Instruction, Xcm};
	}
	pub mod v5 {
		// Everything from v4
		pub use crate::v5::*;
		// Then override with the opaque types in v5
		pub use crate::v5::opaque::{Instruction, Xcm};
	}

	pub mod latest {
		pub use super::v5::*;
	}

	pub mod lts {
		pub use super::v4::*;
	}

	/// The basic `VersionedXcm` type which just uses the `Vec<u8>` as an encoded call.
	pub type VersionedXcm = super::VersionedXcm<()>;
}

#[test]
fn conversion_works() {
	use latest::prelude::*;
	let assets: Assets = (Here, 1u128).into();
	let _: VersionedAssets = assets.into();
}

#[test]
fn size_limits() {
	extern crate std;

	let mut test_failed = false;
	macro_rules! check_sizes {
        ($(($kind:ty, $expected:expr),)+) => {
            $({
                let s = core::mem::size_of::<$kind>();
                // Since the types often affect the size of other types in which they're included
                // it is more convenient to check multiple types at the same time and only fail
                // the test at the end. For debugging it's also useful to print out all of the sizes,
                // even if they're within the expected range.
                if s > $expected {
                    test_failed = true;
                    std::eprintln!(
                        "assertion failed: size of '{}' is {} (which is more than the expected {})",
                        stringify!($kind),
                        s,
                        $expected
                    );
                } else {
                    std::println!(
                        "type '{}' is of size {} which is within the expected {}",
                        stringify!($kind),
                        s,
                        $expected
                    );
                }
            })+
        }
    }

	check_sizes! {
		(crate::latest::Instruction<()>, 128),
		(crate::latest::Asset, 80),
		(crate::latest::Location, 24),
		(crate::latest::AssetId, 40),
		(crate::latest::Junctions, 16),
		(crate::latest::Junction, 88),
		(crate::latest::Response, 40),
		(crate::latest::AssetInstance, 48),
		(crate::latest::NetworkId, 48),
		(crate::latest::BodyId, 32),
		(crate::latest::Assets, 24),
		(crate::latest::BodyPart, 12),
	}
	assert!(!test_failed);
}

#[test]
fn validate_xcm_nesting_works() {
	use crate::latest::{
		prelude::{GeneralIndex, ReserveAssetDeposited, SetAppendix},
		Assets, Xcm, MAX_INSTRUCTIONS_TO_DECODE, MAX_ITEMS_IN_ASSETS,
	};

	// closure generates assets of `count`
	let assets = |count| {
		let mut assets = Assets::new();
		for i in 0..count {
			assets.push((GeneralIndex(i as u128), 100).into());
		}
		assets
	};

	// closer generates `Xcm` with nested instructions of `depth`
	let with_instr = |depth| {
		let mut xcm = Xcm::<()>(vec![]);
		for _ in 0..depth - 1 {
			xcm = Xcm::<()>(vec![SetAppendix(xcm)]);
		}
		xcm
	};

	// `MAX_INSTRUCTIONS_TO_DECODE` check
	assert!(VersionedXcm::<()>::from(Xcm(vec![
		ReserveAssetDeposited(assets(1));
		(MAX_INSTRUCTIONS_TO_DECODE - 1) as usize
	]))
	.validate_xcm_nesting()
	.is_ok());
	assert!(VersionedXcm::<()>::from(Xcm(vec![
		ReserveAssetDeposited(assets(1));
		MAX_INSTRUCTIONS_TO_DECODE as usize
	]))
	.validate_xcm_nesting()
	.is_ok());
	assert!(VersionedXcm::<()>::from(Xcm(vec![
		ReserveAssetDeposited(assets(1));
		(MAX_INSTRUCTIONS_TO_DECODE + 1) as usize
	]))
	.validate_xcm_nesting()
	.is_err());

	// `MAX_XCM_DECODE_DEPTH` check
	assert!(VersionedXcm::<()>::from(with_instr(MAX_XCM_DECODE_DEPTH - 1))
		.validate_xcm_nesting()
		.is_ok());
	assert!(VersionedXcm::<()>::from(with_instr(MAX_XCM_DECODE_DEPTH))
		.validate_xcm_nesting()
		.is_ok());
	assert!(VersionedXcm::<()>::from(with_instr(MAX_XCM_DECODE_DEPTH + 1))
		.validate_xcm_nesting()
		.is_err());

	// `MAX_ITEMS_IN_ASSETS` check
	assert!(VersionedXcm::<()>::from(Xcm(vec![ReserveAssetDeposited(assets(
		MAX_ITEMS_IN_ASSETS
	))]))
	.validate_xcm_nesting()
	.is_ok());
	assert!(VersionedXcm::<()>::from(Xcm(vec![ReserveAssetDeposited(assets(
		MAX_ITEMS_IN_ASSETS - 1
	))]))
	.validate_xcm_nesting()
	.is_ok());
	assert!(VersionedXcm::<()>::from(Xcm(vec![ReserveAssetDeposited(assets(
		MAX_ITEMS_IN_ASSETS + 1
	))]))
	.validate_xcm_nesting()
	.is_err());
}
