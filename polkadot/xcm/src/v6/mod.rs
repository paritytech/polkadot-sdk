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

//! Version 6 of the Cross-Consensus Message format data structures.

pub use super::v3::GetWeight;
use super::v5::{Instruction as OldInstruction, Xcm as OldXcm};
use codec::{self, Decode, Encode};
use core::result;
use educe::Educe;
use scale_info::TypeInfo;

use crate::traits::IntoInstruction;

pub mod instruction;
pub mod instructions;

use crate::{apply_instructions, impl_xcm_instruction};
pub use instructions::*;

pub use super::v5::{
	Ancestor, AncestorThen, Asset, AssetFilter, AssetId, AssetInstance, AssetTransferFilter,
	Assets, BodyId, BodyPart, Error, Fungibility, Hint, HintNumVariants, InteriorLocation,
	Junction, Junctions, Location, MaxDispatchErrorLen, MaybeErrorCode, NetworkId, OriginKind,
	Outcome, Parent, ParentThen, PreparedMessage, QueryId, QueryResponseInfo, Response, Result,
	SendError, Weight, WeightLimit, WildAsset, WildFungibility, XcmContext, XcmHash,
	MAX_ITEMS_IN_ASSETS, ROCOCO_GENESIS_HASH, WESTEND_GENESIS_HASH,
};

pub const VERSION: super::Version = 6;

pub type Xcm<Call> = crate::XcmBase<Instruction<Call>>;

/// A prelude for importing all types typically used when interacting with XCM messages.
pub mod prelude {
	mod contents {
		pub use super::super::VERSION as XCM_VERSION;
		pub use crate::v5::prelude::{
			Ancestor, AncestorThen, Asset,
			AssetFilter::{self, *},
			AssetId,
			AssetInstance::{self, *},
			Assets, BodyId, BodyPart,
			Fungibility::{self, *},
			Hint::{self, *},
			HintNumVariants, InteriorLocation,
			Junction::{self, *},
			Junctions::{self, Here},
			Location, MaybeErrorCode,
			NetworkId::{self, *},
			OriginKind, Outcome, PalletInfo, Parent, ParentThen, PreparedMessage, QueryId,
			QueryResponseInfo, Reanchorable, Response, SendError, Weight,
			WeightLimit::{self, *},
			WildAsset::{self, *},
			WildFungibility::{self, Fungible as WildFungible, NonFungible as WildNonFungible},
			XcmContext, XcmError, XcmHash, XcmResult, XcmWeightInfo,
		};
	}
	pub use crate::traits::{ExecuteXcm, SendXcm, SendResult, validate_send, send_xcm};
	pub use super::instructions::*;
	pub use super::{Instruction, Xcm};
	pub use contents::*;
	pub mod opaque {
		pub use super::{
			super::opaque::{Instruction, Xcm},
			contents::*,
		};
	}
}

apply_instructions!(impl_xcm_instruction, pub enum Instruction<Call>);

impl<Call> Xcm<Call> {
	pub fn into<C>(self) -> Xcm<C> {
		Xcm::from(self)
	}
	pub fn from<C>(xcm: Xcm<C>) -> Self {
		Self(xcm.0.into_iter().map(Instruction::<Call>::from).collect())
	}
}

pub mod opaque {
	/// The basic concrete type of `Xcm`, which doesn't make any assumptions about the
	/// format of a call other than it is pre-encoded.
	pub type Xcm = super::Xcm<()>;

	/// The basic concrete type of `Instruction`, which doesn't make any assumptions about the
	/// format of a call other than it is pre-encoded.
	pub type Instruction = super::Instruction<()>;
}

// Convert from a v5 XCM to a v6 XCM
impl<Call: 'static> TryFrom<OldXcm<Call>> for Xcm<Call> {
	type Error = ();
	fn try_from(old_xcm: OldXcm<Call>) -> result::Result<Self, Self::Error> {
		Ok(Self(old_xcm.0.into_iter().map(TryInto::try_into).collect::<result::Result<_, _>>()?))
	}
}

// Convert from a v5 instruction to a v6 instruction
impl<Call> TryFrom<OldInstruction<Call>> for Instruction<Call> {
	type Error = ();
	fn try_from(old_instruction: OldInstruction<Call>) -> result::Result<Self, Self::Error> {
		Ok(match old_instruction {
			OldInstruction::WithdrawAsset(assets) => WithdrawAsset(assets).into_instruction(),
			OldInstruction::ReserveAssetDeposited(assets) => {
				ReserveAssetDeposited(assets).into_instruction()
			},
			OldInstruction::ReceiveTeleportedAsset(assets) => {
				ReceiveTeleportedAsset(assets).into_instruction()
			},
			OldInstruction::QueryResponse { query_id, response, max_weight, querier } => {
				QueryResponse { query_id, response, max_weight, querier }.into_instruction()
			},
			OldInstruction::TransferAsset { assets, beneficiary } => {
				TransferAsset { assets, beneficiary }.into_instruction()
			},
			OldInstruction::TransferReserveAsset { assets, dest, xcm } => {
				TransferReserveAsset { assets, dest, xcm: xcm.try_into()? }.into_instruction()
			},
			OldInstruction::Transact { origin_kind, fallback_max_weight, call } => {
				Transact { origin_kind, fallback_max_weight, call }.into_instruction()
			},
			OldInstruction::HrmpNewChannelOpenRequest {
				sender,
				max_message_size,
				max_capacity,
			} => HrmpNewChannelOpenRequest { sender, max_message_size, max_capacity }
				.into_instruction(),
			OldInstruction::HrmpChannelAccepted { recipient } => {
				HrmpChannelAccepted { recipient }.into_instruction()
			},
			OldInstruction::HrmpChannelClosing { initiator, sender, recipient } => {
				HrmpChannelClosing { initiator, sender, recipient }.into_instruction()
			},
			OldInstruction::ClearOrigin => ClearOrigin.into_instruction(),
			OldInstruction::DescendOrigin(junctions) => DescendOrigin(junctions).into_instruction(),
			OldInstruction::ReportError(query_response_info) => {
				ReportError(query_response_info).into_instruction()
			},
			OldInstruction::DepositAsset { assets, beneficiary } => {
				DepositAsset { assets, beneficiary }.into_instruction()
			},
			OldInstruction::DepositReserveAsset { assets, dest, xcm } => {
				DepositReserveAsset { assets, dest, xcm: xcm.try_into()? }.into_instruction()
			},
			OldInstruction::ExchangeAsset { give, want, maximal } => {
				ExchangeAsset { give, want, maximal }.into_instruction()
			},
			OldInstruction::InitiateReserveWithdraw { assets, reserve, xcm } => {
				InitiateReserveWithdraw { assets, reserve, xcm: xcm.try_into()? }.into_instruction()
			},
			OldInstruction::InitiateTeleport { assets, dest, xcm } => {
				InitiateTeleport { assets, dest, xcm: xcm.try_into()? }.into_instruction()
			},
			OldInstruction::ReportHolding { response_info, assets } => {
				ReportHolding { response_info, assets }.into_instruction()
			},
			OldInstruction::BuyExecution { fees, weight_limit } => {
				BuyExecution { fees, weight_limit }.into_instruction()
			},
			OldInstruction::RefundSurplus => RefundSurplus.into_instruction(),
			OldInstruction::SetErrorHandler(xcm) => {
				SetErrorHandler(xcm.try_into()?).into_instruction()
			},
			OldInstruction::SetAppendix(xcm) => SetAppendix(xcm.try_into()?).into_instruction(),
			OldInstruction::ClearError => ClearError.into_instruction(),
			OldInstruction::ClaimAsset { assets, ticket } => {
				ClaimAsset { assets, ticket }.into_instruction()
			},
			OldInstruction::Trap(_) => Trap(0).into_instruction(),
			OldInstruction::SubscribeVersion { query_id, max_response_weight } => {
				SubscribeVersion { query_id, max_response_weight }.into_instruction()
			},
			OldInstruction::UnsubscribeVersion => UnsubscribeVersion.into_instruction(),
			OldInstruction::BurnAsset(assets) => BurnAsset(assets).into_instruction(),
			OldInstruction::ExpectAsset(assets) => ExpectAsset(assets).into_instruction(),
			OldInstruction::ExpectOrigin(location) => ExpectOrigin(location).into_instruction(),
			OldInstruction::ExpectError(_) => ExpectError(None).into_instruction(),
			OldInstruction::ExpectTransactStatus(maybe_error_code) => {
				ExpectTransactStatus(maybe_error_code).into_instruction()
			},
			OldInstruction::QueryPallet { module_name, response_info } => {
				QueryPallet { module_name, response_info }.into_instruction()
			},
			OldInstruction::ExpectPallet {
				index,
				name,
				module_name,
				crate_major,
				min_crate_minor,
			} => ExpectPallet { index, name, module_name, crate_major, min_crate_minor }
				.into_instruction(),
			OldInstruction::ReportTransactStatus(query_response_info) => {
				ReportTransactStatus(query_response_info).into_instruction()
			},
			OldInstruction::ClearTransactStatus => ClearTransactStatus.into_instruction(),
			OldInstruction::UniversalOrigin(junction) => {
				UniversalOrigin(junction).into_instruction()
			},
			OldInstruction::ExportMessage { network, destination, xcm } => {
				ExportMessage { network, destination, xcm: xcm.try_into()? }.into_instruction()
			},
			OldInstruction::LockAsset { asset, unlocker } => {
				LockAsset { asset, unlocker }.into_instruction()
			},
			OldInstruction::UnlockAsset { asset, target } => {
				UnlockAsset { asset, target }.into_instruction()
			},
			OldInstruction::NoteUnlockable { asset, owner } => {
				NoteUnlockable { asset, owner }.into_instruction()
			},
			OldInstruction::RequestUnlock { asset, locker } => {
				RequestUnlock { asset, locker }.into_instruction()
			},
			OldInstruction::SetFeesMode { jit_withdraw } => {
				SetFeesMode { jit_withdraw }.into_instruction()
			},
			OldInstruction::SetTopic(topic) => SetTopic(topic).into_instruction(),
			OldInstruction::ClearTopic => ClearTopic.into_instruction(),
			OldInstruction::AliasOrigin(location) => AliasOrigin(location).into_instruction(),
			OldInstruction::UnpaidExecution { weight_limit, check_origin } => {
				UnpaidExecution { weight_limit, check_origin }.into_instruction()
			},
			OldInstruction::PayFees { asset } => PayFees { asset }.into_instruction(),
			OldInstruction::InitiateTransfer {
				destination,
				remote_fees,
				preserve_origin,
				assets,
				remote_xcm,
			} => InitiateTransfer {
				destination,
				remote_fees,
				preserve_origin,
				assets,
				remote_xcm: remote_xcm.try_into()?,
			}
			.into_instruction(),
			OldInstruction::ExecuteWithOrigin { descendant_origin, xcm } => {
				ExecuteWithOrigin { descendant_origin, xcm: xcm.try_into()? }.into_instruction()
			},
			OldInstruction::SetHints { hints } => SetHints { hints }.into_instruction(),
		})
	}
}
