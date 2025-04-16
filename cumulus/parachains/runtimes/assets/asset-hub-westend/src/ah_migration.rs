// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use super::*;
use codec::DecodeAll;
use frame_support::pallet_prelude::TypeInfo;
// use parachains_common::pay::VersionedLocatableAccount;
use polkadot_runtime_common::impls::{LocatableAssetConverter, VersionedLocatableAsset};
use sp_runtime::traits::{Convert, TryConvert};
use xcm::latest::prelude::*;

/// Relay Chain Hold Reason
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum RcHoldReason {
	#[codec(index = 28u8)]
	Preimage(pallet_preimage::HoldReason),
}

impl Default for RcHoldReason {
	fn default() -> Self {
		RcHoldReason::Preimage(pallet_preimage::HoldReason::Preimage)
	}
}

/// Relay Chain Freeze Reason
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum RcFreezeReason {
	#[codec(index = 29u8)]
	NominationPools(pallet_nomination_pools::FreezeReason),
}

impl Default for RcFreezeReason {
	fn default() -> Self {
		RcFreezeReason::NominationPools(pallet_nomination_pools::FreezeReason::PoolMinBalance)
	}
}

#[derive(DecodeWithMemTracking, Decode)]
pub struct RcToAhHoldReason;
impl Convert<RcHoldReason, RuntimeHoldReason> for RcToAhHoldReason {
	fn convert(_: RcHoldReason) -> RuntimeHoldReason {
		PreimageHoldReason::get()
	}
}

#[derive(DecodeWithMemTracking, Decode)]
pub struct RcToAhFreezeReason;
impl Convert<RcFreezeReason, RuntimeFreezeReason> for RcToAhFreezeReason {
	fn convert(reason: RcFreezeReason) -> RuntimeFreezeReason {
		match reason {
			RcFreezeReason::NominationPools(
				pallet_nomination_pools::FreezeReason::PoolMinBalance,
			) => RuntimeFreezeReason::NominationPools(
				pallet_nomination_pools::FreezeReason::PoolMinBalance,
			),
		}
	}
}

/// Relay Chain Proxy Type
///
/// Coped from https://github.com/polkadot-fellows/runtimes/blob/dde99603d7dbd6b8bf541d57eb30d9c07a4fce32/relay/polkadot/src/lib.rs#L986-L1010
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, Default,
)]
pub enum RcProxyType {
	#[default]
	Any = 0,
	NonTransfer = 1,
	Governance = 2,
	Staking = 3,
	SudoBalances = 4,
	IdentityJudgement = 5,
	CancelProxy = 6,
	Auction = 7,
	NominationPools = 8,
	ParaRegistration = 9,
}

pub struct RcToProxyType;
impl TryConvert<RcProxyType, ProxyType> for RcToProxyType {
	fn try_convert(p: RcProxyType) -> Result<ProxyType, RcProxyType> {
		match p {
			RcProxyType::Any => Ok(ProxyType::Any),
			RcProxyType::NonTransfer => Ok(ProxyType::NonTransfer),
			RcProxyType::Governance => Ok(ProxyType::Governance),
			RcProxyType::Staking => Ok(ProxyType::Staking),
			RcProxyType::SudoBalances => Err(p), // Does not exist on AH
			RcProxyType::IdentityJudgement => Err(p), // Does not exist on AH
			RcProxyType::CancelProxy => Ok(ProxyType::CancelProxy),
			RcProxyType::Auction => Err(p), // Does not exist on AH
			RcProxyType::NominationPools => Ok(ProxyType::NominationPools),
			RcProxyType::ParaRegistration => Err(p), // Does not exist on AH
		}
	}
}

/// A subset of Relay Chain origins.
///
/// These origins are utilized in Governance and mapped to Asset Hub origins for active referendums.
#[allow(non_camel_case_types)]
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub enum RcPalletsOrigin {
	#[codec(index = 0u8)]
	system(frame_system::Origin<Runtime>),
	#[codec(index = 22u8)]
	Origins(pallet_custom_origins::Origin),
}

impl Default for RcPalletsOrigin {
	fn default() -> Self {
		RcPalletsOrigin::system(frame_system::Origin::<Runtime>::Root)
	}
}

/// Convert a Relay Chain origin to an Asset Hub one.
pub struct RcToAhPalletsOrigin;
impl TryConvert<RcPalletsOrigin, OriginCaller> for RcToAhPalletsOrigin {
	fn try_convert(a: RcPalletsOrigin) -> Result<OriginCaller, RcPalletsOrigin> {
		match a {
			RcPalletsOrigin::system(a) => Ok(OriginCaller::system(a)),
			RcPalletsOrigin::Origins(a) => Ok(OriginCaller::Origins(a)),
		}
	}
}

/// Relay Chain Runtime Call.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub enum RcRuntimeCall {
	// TODO: variant set code for Relay Chain
	// TODO: variant set code for Parachains
	// TODO: whitelisted caller
	#[codec(index = 0u8)]
	System(frame_system::Call<Runtime>),
	#[codec(index = 19u8)]
	Treasury(RcTreasuryCall),
	#[codec(index = 21u8)]
	Referenda(pallet_referenda::Call<Runtime>),
	#[codec(index = 26u8)]
	Utility(RcUtilityCall),
}

/// Relay Chain Treasury Call obtained from cargo expand.
#[allow(non_camel_case_types)]
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub enum RcTreasuryCall {
	/// Propose and approve a spend of treasury funds.
	#[codec(index = 3u8)]
	spend_local {
		#[codec(compact)]
		amount: Balance,
		beneficiary: Address,
	},
	/// Force a previously approved proposal to be removed from the approval queue.
	#[codec(index = 4u8)]
	remove_approval {
		#[codec(compact)]
		proposal_id: pallet_treasury::ProposalIndex,
	},
	/// Propose and approve a spend of treasury funds.
	#[codec(index = 5u8)]
	spend {
		asset_kind: Box<VersionedLocatableAsset>,
		#[codec(compact)]
		amount: Balance,
		beneficiary: VersionedLocation,
		valid_from: Option<BlockNumber>,
	},
	/// Claim a spend.
	#[codec(index = 6u8)]
	payout { index: pallet_treasury::SpendIndex },
	#[codec(index = 7u8)]
	check_status { index: pallet_treasury::SpendIndex },
	#[codec(index = 8u8)]
	void_spend { index: pallet_treasury::SpendIndex },
}

/// Relay Chain Utility Call obtained from cargo expand.
///
/// The variants that are not generally used in Governance are not included.
#[allow(non_camel_case_types)]
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub enum RcUtilityCall {
	/// Send a batch of dispatch calls.
	#[codec(index = 0u8)]
	batch { calls: Vec<RcRuntimeCall> },
	/// Send a batch of dispatch calls and atomically execute them.
	#[codec(index = 2u8)]
	batch_all { calls: Vec<RcRuntimeCall> },
	/// Dispatches a function call with a provided origin.
	#[codec(index = 3u8)]
	dispatch_as { as_origin: Box<RcPalletsOrigin>, call: Box<RcRuntimeCall> },
	/// Send a batch of dispatch calls.
	/// Unlike `batch`, it allows errors and won't interrupt.
	#[codec(index = 4u8)]
	force_batch { calls: Vec<RcRuntimeCall> },
}

/// Convert an encoded Relay Chain Call to a local AH one.
pub struct RcToAhCall;
impl<'a> TryConvert<&'a [u8], RuntimeCall> for RcToAhCall {
	fn try_convert(mut a: &'a [u8]) -> Result<RuntimeCall, &'a [u8]> {
		let rc_call = match RcRuntimeCall::decode_all(&mut a) {
			Ok(rc_call) => rc_call,
			Err(e) => {
				log::error!("Failed to decode RC call with error: {:?}", e);
				return Err(a)
			},
		};
		Self::map(rc_call).map_err(|_| a)
	}
}
impl RcToAhCall {
	fn map(rc_call: RcRuntimeCall) -> Result<RuntimeCall, ()> {
		match rc_call {
			RcRuntimeCall::System(inner_call) => {
				let call =
					inner_call.using_encoded(|mut e| Decode::decode(&mut e)).map_err(|err| {
						log::error!(
							target: LOG_TARGET,
							"Failed to decode RC Bounties call to AH System call: {:?}",
							err
						);
					})?;
				Ok(RuntimeCall::System(call))
			},
			RcRuntimeCall::Utility(RcUtilityCall::dispatch_as { as_origin, call }) => {
				let origin = RcToAhPalletsOrigin::try_convert(*as_origin).map_err(|err| {
					log::error!("Failed to decode RC dispatch_as origin: {:?}", err);
				})?;
				Ok(RuntimeCall::Utility(pallet_utility::Call::<Runtime>::dispatch_as {
					as_origin: Box::new(origin),
					call: Box::new(Self::map(*call)?),
				}))
			},
			RcRuntimeCall::Utility(RcUtilityCall::batch { calls }) =>
				Ok(RuntimeCall::Utility(pallet_utility::Call::<Runtime>::batch {
					calls: calls
						.into_iter()
						.map(|c| Self::map(c))
						.collect::<Result<Vec<_>, _>>()?,
				})),
			RcRuntimeCall::Utility(RcUtilityCall::batch_all { calls }) =>
				Ok(RuntimeCall::Utility(pallet_utility::Call::<Runtime>::batch_all {
					calls: calls
						.into_iter()
						.map(|c| Self::map(c))
						.collect::<Result<Vec<_>, _>>()?,
				})),
			RcRuntimeCall::Utility(RcUtilityCall::force_batch { calls }) =>
				Ok(RuntimeCall::Utility(pallet_utility::Call::<Runtime>::force_batch {
					calls: calls
						.into_iter()
						.map(|c| Self::map(c))
						.collect::<Result<Vec<_>, _>>()?,
				})),
			RcRuntimeCall::Treasury(RcTreasuryCall::spend {
				asset_kind,
				amount,
				beneficiary,
				valid_from,
			}) => {
				let asset_kind =
					LocatableAssetConverter::try_convert(*asset_kind).map_err(|_| {
						log::error!("Failed to convert RC asset kind to latest version");
					})?;
				if asset_kind.location != Location::new(0, Parachain(1000)) {
					log::error!("Unsupported RC asset kind location: {:?}", asset_kind.location);
					return Err(());
				};
				let asset_kind = VersionedLocatableAsset::V5 {
					location: Location::here(),
					asset_id: asset_kind.asset_id,
				};
				let beneficiary = beneficiary.try_into().map_err(|_| {
					log::error!("Failed to convert RC beneficiary type to the latest version");
				})?;
				Ok(RuntimeCall::Treasury(pallet_treasury::Call::<Runtime>::spend {
					asset_kind: Box::new(asset_kind),
					amount,
					beneficiary: Box::new(beneficiary),
					valid_from,
				}))
			},
			RcRuntimeCall::Treasury(inner_call) => {
				let call =
					inner_call.using_encoded(|mut e| Decode::decode(&mut e)).map_err(|err| {
						log::error!("Failed to decode inner RC call into inner AH call: {:?}", err);
						()
					})?;
				Ok(RuntimeCall::Treasury(call))
			},
			RcRuntimeCall::Referenda(inner_call) => {
				let call =
					inner_call.using_encoded(|mut e| Decode::decode(&mut e)).map_err(|err| {
						log::error!(
							"Failed to decode RC Referenda call to AH Referenda call: {:?}",
							err
						);
						()
					})?;
				Ok(RuntimeCall::Referenda(call))
			},
		}
	}
}
