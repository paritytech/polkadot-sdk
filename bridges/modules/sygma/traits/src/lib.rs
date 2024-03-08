// The Licensed Work is (c) 2022 Sygma
// SPDX-License-Identifier: LGPL-3.0-only

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::dispatch::DispatchResult;
use primitive_types::{H160, U256};
use scale_info::TypeInfo;
use sp_std::vec::Vec;
use xcm::latest::prelude::*;
use xcm::opaque::v3::{MultiLocation, MultiAsset};

pub type DomainID = u8;
pub type DepositNonce = u64;
pub type ResourceId = [u8; 32];
pub type ChainID = U256;
pub type VerifyingContractAddress = H160;

#[derive(Clone, Debug, Eq, PartialEq, Encode, Decode, TypeInfo)]
pub enum TransferType {
	FungibleTransfer,
	NonFungibleTransfer,
	GenericTransfer,
}

#[derive(
	Clone,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Debug,
	Encode,
	Decode,
	TypeInfo,
	Copy,
	Default,
	MaxEncodedLen,
)]
pub struct MpcAddress(pub [u8; 20]);

pub trait ExtractDestinationData {
	fn extract_dest(dest: &MultiLocation) -> Option<(Vec<u8>, DomainID)>;
}

pub trait FeeHandler {
	// Return fee represent by a specific asset
	fn get_fee(domain: DomainID, asset: MultiAsset) -> Option<u128>;
}

impl FeeHandler for () {
	fn get_fee(_domain: DomainID, _asset: MultiAsset) -> Option<u128> {
		None
	}
}

pub trait DecimalConverter {
	/// convert_to converts the MultiAsset to u128 when bridging from sygma substrate pallet.
	/// Sygma relayer will always expect asset in 18 decimal
	fn convert_to(asset: &MultiAsset) -> Option<u128>;
	/// convert_from converts a u128 to MultiAsset when bridging to sygma substrate pallet.
	/// Sygma relayer will always send asset in 18 decimal
	fn convert_from(asset: &MultiAsset) -> Option<MultiAsset>;
}

// when integrating with parachain, parachain team can implement their own version
pub trait AssetTypeIdentifier {
	fn is_native_asset(asset: &MultiAsset) -> bool;
}

pub trait TransactorForwarder {
	fn xcm_transactor_forwarder(
		sender: [u8; 32],
		what: MultiAsset,
		dest: MultiLocation,
	) -> DispatchResult;
	fn other_world_transactor_forwarder(
		sender: [u8; 32],
		what: MultiAsset,
		dest: MultiLocation,
	) -> DispatchResult;
}

pub trait Bridge {
	fn transfer(
		sender: [u8; 32],
		asset: MultiAsset,
		dest: MultiLocation,
		max_weight: Option<Weight>,
	) -> DispatchResult;
}

pub trait AssetReserveLocationParser {
	fn reserved_location(asset: &MultiAsset) -> Option<MultiLocation>;
}
