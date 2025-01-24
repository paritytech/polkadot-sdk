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
use bounded_collections::BoundedVec;
use codec::{self, Decode, Encode};
use core::result;
use educe::Educe;
use scale_info::TypeInfo;
use alloc::vec::Vec;

use crate::{traits::IntoInstruction, DoubleEncoded};

pub mod instruction;
pub mod instructions;

use crate::{apply_instructions, impl_xcm_instruction};
use instructions::*;

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
		pub use super::super::{VERSION as XCM_VERSION, XcmWeightInfo, instructions};
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
			XcmContext, XcmError, XcmHash, XcmResult,
		};
		pub use crate::v6::Instruction::{self, *};
	}
	pub use super::{InstructionsV6, Xcm};
	pub use crate::traits::{send_xcm, validate_send, ExecuteXcm, SendResult, SendXcm};
	pub use contents::*;
	pub mod opaque {
		pub use super::{
			super::opaque::{Instruction, Xcm},
			contents::*,
		};
	}
}

apply_instructions!(impl_xcm_instruction, pub enum InstructionsV6<Call>);

#[derive(Educe, Encode, Decode, TypeInfo, xcm_procedural::XcmWeightInfoTrait)]
#[educe(Clone(bound = false), Eq, PartialEq(bound = false), Debug(bound = false))]
#[codec(encode_bound())]
#[codec(decode_bound())]
#[scale_info(bounds(), skip_type_params(Call))]
pub enum Instruction<Call: 'static> {
	WithdrawAsset(Assets),
	ReserveAssetDeposited(Assets),
	ReceiveTeleportedAsset(Assets),
	QueryResponse {
		#[codec(compact)]
		query_id: QueryId,
		response: Response,
		max_weight: Weight,
		querier: Option<Location>,
	},
	TransferAsset {
		assets: Assets,
		beneficiary: Location,
	},
	TransferReserveAsset {
		assets: Assets,
		dest: Location,
		xcm: Xcm<()>,
	},
	Transact {
		origin_kind: OriginKind,
		fallback_max_weight: Option<Weight>,
		call: DoubleEncoded<Call>,
	},
	HrmpNewChannelOpenRequest {
		#[codec(compact)]
		sender: u32,
		#[codec(compact)]
		max_message_size: u32,
		#[codec(compact)]
		max_capacity: u32,
	},
	HrmpChannelAccepted {
		#[codec(compact)]
		recipient: u32,
	},
	HrmpChannelClosing {
		#[codec(compact)]
		initiator: u32,
		#[codec(compact)]
		sender: u32,
		#[codec(compact)]
		recipient: u32,
	},
	ClearOrigin,
	DescendOrigin(InteriorLocation),
	ReportError(QueryResponseInfo),
	DepositAsset {
		assets: AssetFilter,
		beneficiary: Location,
	},
	DepositReserveAsset {
		assets: AssetFilter,
		dest: Location,
		xcm: Xcm<()>,
	},
	ExchangeAsset {
		give: AssetFilter,
		want: Assets,
		maximal: bool,
	},
	InitiateReserveWithdraw {
		assets: AssetFilter,
		reserve: Location,
		xcm: Xcm<()>,
	},
	InitiateTeleport {
		assets: AssetFilter,
		dest: Location,
		xcm: Xcm<()>,
	},
	ReportHolding {
		response_info: QueryResponseInfo,
		assets: AssetFilter,
	},
	BuyExecution {
		fees: Asset,
		weight_limit: WeightLimit,
	},
	RefundSurplus,
	SetErrorHandler(Xcm<Call>),
	SetAppendix(Xcm<Call>),
	ClearError,
	ClaimAsset {
		assets: Assets,
		ticket: Location,
	},
	Trap(#[codec(compact)] u64),
	SubscribeVersion {
		#[codec(compact)]
		query_id: QueryId,
		max_response_weight: Weight,
	},
	UnsubscribeVersion,
	BurnAsset(Assets),
	ExpectAsset(Assets),
	ExpectOrigin(Option<Location>),
	ExpectError(Option<(u32, Error)>),
	ExpectTransactStatus(MaybeErrorCode),
	QueryPallet {
		module_name: Vec<u8>,
		response_info: QueryResponseInfo,
	},
	ExpectPallet {
		#[codec(compact)]
		index: u32,
		name: Vec<u8>,
		module_name: Vec<u8>,
		#[codec(compact)]
		crate_major: u32,
		#[codec(compact)]
		min_crate_minor: u32,
	},
	ReportTransactStatus(QueryResponseInfo),
	ClearTransactStatus,
	UniversalOrigin(Junction),
	ExportMessage {
		network: NetworkId,
		destination: InteriorLocation,
		xcm: Xcm<()>,
	},
	LockAsset {
		asset: Asset,
		unlocker: Location,
	},
	UnlockAsset {
		asset: Asset,
		target: Location,
	},
	NoteUnlockable {
		asset: Asset,
		owner: Location,
	},
	RequestUnlock {
		asset: Asset,
		locker: Location,
	},
	SetFeesMode {
		jit_withdraw: bool,
	},
	SetTopic([u8; 32]),
	ClearTopic,
	AliasOrigin(Location),
	UnpaidExecution {
		weight_limit: WeightLimit,
		check_origin: Option<Location>,
	},
	PayFees {
		asset: Asset,
	},
	InitiateTransfer {
		destination: Location,
		remote_fees: Option<AssetTransferFilter>,
		preserve_origin: bool,
		assets: Vec<AssetTransferFilter>,
		remote_xcm: Xcm<()>,
	},
	ExecuteWithOrigin {
		descendant_origin: Option<InteriorLocation>,
		xcm: Xcm<Call>,
	},
	SetHints {
		hints: BoundedVec<Hint, HintNumVariants>,
	},
}

impl<Call> From<Instruction<Call>> for InstructionsV6<Call> {
	fn from(value: Instruction<Call>) -> Self {
		match value {
			Instruction::WithdrawAsset(assets) => WithdrawAsset(assets).into_instruction(),
			Instruction::ReserveAssetDeposited(assets) => {
				ReserveAssetDeposited(assets).into_instruction()
			},
			Instruction::ReceiveTeleportedAsset(assets) => {
				ReceiveTeleportedAsset(assets).into_instruction()
			},
			Instruction::QueryResponse { query_id, response, max_weight, querier } => {
				QueryResponse { query_id, response, max_weight, querier }.into_instruction()
			},
			Instruction::TransferAsset { assets, beneficiary } => {
				TransferAsset { assets, beneficiary }.into_instruction()
			},
			Instruction::TransferReserveAsset { assets, dest, xcm } => {
				TransferReserveAsset { assets, dest, xcm }.into_instruction()
			},
			Instruction::Transact { origin_kind, fallback_max_weight, call } => {
				Transact { origin_kind, fallback_max_weight, call }.into_instruction()
			},
			Instruction::HrmpNewChannelOpenRequest { sender, max_message_size, max_capacity } => {
				HrmpNewChannelOpenRequest { sender, max_message_size, max_capacity }
					.into_instruction()
			},
			Instruction::HrmpChannelAccepted { recipient } => {
				HrmpChannelAccepted { recipient }.into_instruction()
			},
			Instruction::HrmpChannelClosing { initiator, sender, recipient } => {
				HrmpChannelClosing { initiator, sender, recipient }.into_instruction()
			},
			Instruction::ClearOrigin => ClearOrigin.into_instruction(),
			Instruction::DescendOrigin(junctions) => DescendOrigin(junctions).into_instruction(),
			Instruction::ReportError(query_response_info) => {
				ReportError(query_response_info).into_instruction()
			},
			Instruction::DepositAsset { assets, beneficiary } => {
				DepositAsset { assets, beneficiary }.into_instruction()
			},
			Instruction::DepositReserveAsset { assets, dest, xcm } => {
				DepositReserveAsset { assets, dest, xcm }.into_instruction()
			},
			Instruction::ExchangeAsset { give, want, maximal } => {
				ExchangeAsset { give, want, maximal }.into_instruction()
			},
			Instruction::InitiateReserveWithdraw { assets, reserve, xcm } => {
				InitiateReserveWithdraw { assets, reserve, xcm }.into_instruction()
			},
			Instruction::InitiateTeleport { assets, dest, xcm } => {
				InitiateTeleport { assets, dest, xcm }.into_instruction()
			},
			Instruction::ReportHolding { response_info, assets } => {
				ReportHolding { response_info, assets }.into_instruction()
			},
			Instruction::BuyExecution { fees, weight_limit } => {
				BuyExecution { fees, weight_limit }.into_instruction()
			},
			Instruction::RefundSurplus => RefundSurplus.into_instruction(),
			Instruction::SetErrorHandler(xcm) => SetErrorHandler(xcm).into_instruction(),
			Instruction::SetAppendix(xcm) => SetAppendix(xcm).into_instruction(),
			Instruction::ClearError => ClearError.into_instruction(),
			Instruction::ClaimAsset { assets, ticket } => {
				ClaimAsset { assets, ticket }.into_instruction()
			},
			Instruction::Trap(code) => Trap(code).into_instruction(),
			Instruction::SubscribeVersion { query_id, max_response_weight } => {
				SubscribeVersion { query_id, max_response_weight }.into_instruction()
			},
			Instruction::UnsubscribeVersion => UnsubscribeVersion.into_instruction(),
			Instruction::BurnAsset(assets) => BurnAsset(assets).into_instruction(),
			Instruction::ExpectAsset(assets) => ExpectAsset(assets).into_instruction(),
			Instruction::ExpectOrigin(location) => ExpectOrigin(location).into_instruction(),
			Instruction::ExpectError(_) => ExpectError(None).into_instruction(),
			Instruction::ExpectTransactStatus(maybe_error_code) => {
				ExpectTransactStatus(maybe_error_code).into_instruction()
			},
			Instruction::QueryPallet { module_name, response_info } => {
				QueryPallet { module_name, response_info }.into_instruction()
			},
			Instruction::ExpectPallet {
				index,
				name,
				module_name,
				crate_major,
				min_crate_minor,
			} => ExpectPallet { index, name, module_name, crate_major, min_crate_minor }
				.into_instruction(),
			Instruction::ReportTransactStatus(query_response_info) => {
				ReportTransactStatus(query_response_info).into_instruction()
			},
			Instruction::ClearTransactStatus => ClearTransactStatus.into_instruction(),
			Instruction::UniversalOrigin(junction) => UniversalOrigin(junction).into_instruction(),
			Instruction::ExportMessage { network, destination, xcm } => {
				ExportMessage { network, destination, xcm }.into_instruction()
			},
			Instruction::LockAsset { asset, unlocker } => {
				LockAsset { asset, unlocker }.into_instruction()
			},
			Instruction::UnlockAsset { asset, target } => {
				UnlockAsset { asset, target }.into_instruction()
			},
			Instruction::NoteUnlockable { asset, owner } => {
				NoteUnlockable { asset, owner }.into_instruction()
			},
			Instruction::RequestUnlock { asset, locker } => {
				RequestUnlock { asset, locker }.into_instruction()
			},
			Instruction::SetFeesMode { jit_withdraw } => {
				SetFeesMode { jit_withdraw }.into_instruction()
			},
			Instruction::SetTopic(topic) => SetTopic(topic).into_instruction(),
			Instruction::ClearTopic => ClearTopic.into_instruction(),
			Instruction::AliasOrigin(location) => AliasOrigin(location).into_instruction(),
			Instruction::UnpaidExecution { weight_limit, check_origin } => {
				UnpaidExecution { weight_limit, check_origin }.into_instruction()
			},
			Instruction::PayFees { asset } => PayFees { asset }.into_instruction(),
			Instruction::InitiateTransfer {
				destination,
				remote_fees,
				preserve_origin,
				assets,
				remote_xcm,
			} => InitiateTransfer { destination, remote_fees, preserve_origin, assets, remote_xcm }
				.into_instruction(),
			Instruction::ExecuteWithOrigin { descendant_origin, xcm } => {
				ExecuteWithOrigin { descendant_origin, xcm }.into_instruction()
			},
			Instruction::SetHints { hints } => SetHints { hints }.into_instruction(),
		}
	}
}

impl<Call> From<InstructionsV6<Call>> for Instruction<Call> {
	fn from(value: InstructionsV6<Call>) -> Self {
		match value {
			InstructionsV6::WithdrawAsset(WithdrawAsset(assets)) => {
				Instruction::WithdrawAsset(assets)
			},
			InstructionsV6::ReserveAssetDeposited(ReserveAssetDeposited(assets)) => {
				Instruction::ReserveAssetDeposited(assets)
			},
			InstructionsV6::ReceiveTeleportedAsset(ReceiveTeleportedAsset(assets)) => {
				Instruction::ReceiveTeleportedAsset(assets)
			},
			InstructionsV6::QueryResponse(QueryResponse {
				query_id,
				response,
				max_weight,
				querier,
			}) => Instruction::QueryResponse { query_id, response, max_weight, querier },
			InstructionsV6::TransferAsset(TransferAsset { assets, beneficiary }) => {
				Instruction::TransferAsset { assets, beneficiary }
			},
			InstructionsV6::TransferReserveAsset(TransferReserveAsset { assets, dest, xcm }) => {
				Instruction::TransferReserveAsset { assets, dest, xcm }
			},
			InstructionsV6::Transact(Transact { origin_kind, fallback_max_weight, call }) => {
				Instruction::Transact { origin_kind, fallback_max_weight, call }
			},
			InstructionsV6::HrmpNewChannelOpenRequest(HrmpNewChannelOpenRequest {
				sender,
				max_message_size,
				max_capacity,
			}) => Instruction::HrmpNewChannelOpenRequest { sender, max_message_size, max_capacity },
			InstructionsV6::HrmpChannelAccepted(HrmpChannelAccepted { recipient }) => {
				Instruction::HrmpChannelAccepted { recipient }
			},
			InstructionsV6::HrmpChannelClosing(HrmpChannelClosing {
				initiator,
				sender,
				recipient,
			}) => Instruction::HrmpChannelClosing { initiator, sender, recipient },
			InstructionsV6::ClearOrigin(_) => Instruction::ClearOrigin,
			InstructionsV6::DescendOrigin(DescendOrigin(junctions)) => {
				Instruction::DescendOrigin(junctions)
			},
			InstructionsV6::ReportError(ReportError(query_response_info)) => {
				Instruction::ReportError(query_response_info)
			},
			InstructionsV6::DepositAsset(DepositAsset { assets, beneficiary }) => {
				Instruction::DepositAsset { assets, beneficiary }
			},
			InstructionsV6::DepositReserveAsset(DepositReserveAsset { assets, dest, xcm }) => {
				Instruction::DepositReserveAsset { assets, dest, xcm }
			},
			InstructionsV6::ExchangeAsset(ExchangeAsset { give, want, maximal }) => {
				Instruction::ExchangeAsset { give, want, maximal }
			},
			InstructionsV6::InitiateReserveWithdraw(InitiateReserveWithdraw {
				assets,
				reserve,
				xcm,
			}) => Instruction::InitiateReserveWithdraw { assets, reserve, xcm },
			InstructionsV6::InitiateTeleport(InitiateTeleport { assets, dest, xcm }) => {
				Instruction::InitiateTeleport { assets, dest, xcm }
			},
			InstructionsV6::ReportHolding(ReportHolding { response_info, assets }) => {
				Instruction::ReportHolding { response_info, assets }
			},
			InstructionsV6::BuyExecution(BuyExecution { fees, weight_limit }) => {
				Instruction::BuyExecution { fees, weight_limit }
			},
			InstructionsV6::RefundSurplus(_) => Instruction::RefundSurplus,
			InstructionsV6::SetErrorHandler(SetErrorHandler(xcm)) => {
				Instruction::SetErrorHandler(xcm)
			},
			InstructionsV6::SetAppendix(SetAppendix(xcm)) => Instruction::SetAppendix(xcm),
			InstructionsV6::ClearError(_) => Instruction::ClearError,
			InstructionsV6::ClaimAsset(ClaimAsset { assets, ticket }) => {
				Instruction::ClaimAsset { assets, ticket }
			},
			InstructionsV6::Trap(Trap(code)) => Instruction::Trap(code),
			InstructionsV6::SubscribeVersion(SubscribeVersion {
				query_id,
				max_response_weight,
			}) => Instruction::SubscribeVersion { query_id, max_response_weight },
			InstructionsV6::UnsubscribeVersion(_) => Instruction::UnsubscribeVersion,
			InstructionsV6::BurnAsset(BurnAsset(assets)) => Instruction::BurnAsset(assets),
			InstructionsV6::ExpectAsset(ExpectAsset(assets)) => Instruction::ExpectAsset(assets),
			InstructionsV6::ExpectOrigin(ExpectOrigin(location)) => {
				Instruction::ExpectOrigin(location)
			},
			InstructionsV6::ExpectError(ExpectError(error)) => Instruction::ExpectError(error),
			InstructionsV6::ExpectTransactStatus(ExpectTransactStatus(transact_status)) => {
				Instruction::ExpectTransactStatus(transact_status)
			},
			InstructionsV6::QueryPallet(QueryPallet { module_name, response_info }) => {
				Instruction::QueryPallet { module_name, response_info }
			},
			InstructionsV6::ExpectPallet(ExpectPallet {
				index,
				name,
				module_name,
				crate_major,
				min_crate_minor,
			}) => {
				Instruction::ExpectPallet { index, name, module_name, crate_major, min_crate_minor }
			},
			InstructionsV6::ReportTransactStatus(ReportTransactStatus(query_response_info)) => {
				Instruction::ReportTransactStatus(query_response_info)
			},
			InstructionsV6::ClearTransactStatus(_) => Instruction::ClearTransactStatus,
			InstructionsV6::UniversalOrigin(UniversalOrigin(junction)) => {
				Instruction::UniversalOrigin(junction)
			},
			InstructionsV6::ExportMessage(ExportMessage { network, destination, xcm }) => {
				Instruction::ExportMessage { network, destination, xcm }
			},
			InstructionsV6::LockAsset(LockAsset { asset, unlocker }) => {
				Instruction::LockAsset { asset, unlocker }
			},
			InstructionsV6::UnlockAsset(UnlockAsset { asset, target }) => {
				Instruction::UnlockAsset { asset, target }
			},
			InstructionsV6::NoteUnlockable(NoteUnlockable { asset, owner }) => {
				Instruction::NoteUnlockable { asset, owner }
			},
			InstructionsV6::RequestUnlock(RequestUnlock { asset, locker }) => {
				Instruction::RequestUnlock { asset, locker }
			},
			InstructionsV6::SetFeesMode(SetFeesMode { jit_withdraw }) => {
				Instruction::SetFeesMode { jit_withdraw }
			},
			InstructionsV6::SetTopic(SetTopic(topic)) => Instruction::SetTopic(topic),
			InstructionsV6::ClearTopic(_) => Instruction::ClearTopic,
			InstructionsV6::AliasOrigin(AliasOrigin(location)) => {
				Instruction::AliasOrigin(location)
			},
			InstructionsV6::UnpaidExecution(UnpaidExecution { weight_limit, check_origin }) => {
				Instruction::UnpaidExecution { weight_limit, check_origin }
			},
			InstructionsV6::PayFees(PayFees { asset }) => Instruction::PayFees { asset },
			InstructionsV6::InitiateTransfer(InitiateTransfer {
				destination,
				remote_fees,
				preserve_origin,
				assets,
				remote_xcm,
			}) => Instruction::InitiateTransfer {
				destination,
				remote_fees,
				preserve_origin,
				assets,
				remote_xcm,
			},
			InstructionsV6::ExecuteWithOrigin(ExecuteWithOrigin { descendant_origin, xcm }) => {
				Instruction::ExecuteWithOrigin { descendant_origin, xcm }
			},
			InstructionsV6::SetHints(SetHints { hints }) => Instruction::SetHints { hints },
		}
	}
}

impl<Call> Xcm<Call> {
	pub fn into<C>(self) -> Xcm<C> {
		Xcm::from(self)
	}
	pub fn from<C>(xcm: Xcm<C>) -> Self {
		Self(
			xcm.0
				.into_iter()
				.map(|i| {
					let i: InstructionsV6<C> = i.into();
					let i: InstructionsV6<Call> = i.into();
					Instruction::<Call>::from(i)
				})
				.collect(),
		)
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
		let instv6 = match old_instruction {
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
		};
		Ok(Instruction::<Call>::from(instv6))
	}
}

impl<Call> TryFrom<Xcm<Call>> for OldXcm<Call> {
	type Error = ();
	fn try_from(value: Xcm<Call>) -> result::Result<Self, ()> {
		Ok(Self(value.0.into_iter().map(TryInto::try_into).collect::<result::Result<_, _>>()?))
	}
}

impl<Call> TryFrom<Instruction<Call>> for OldInstruction<Call> {
	type Error = ();
	fn try_from(value: Instruction<Call>) -> result::Result<Self, ()> {
		Ok(match value {
			Instruction::WithdrawAsset(assets) => Self::WithdrawAsset(assets),
			Instruction::ReserveAssetDeposited(assets) => Self::ReserveAssetDeposited(assets),
			Instruction::ReceiveTeleportedAsset(assets) => Self::ReceiveTeleportedAsset(assets),
			Instruction::QueryResponse { query_id, response, max_weight, querier } => {
				Self::QueryResponse { query_id, response, max_weight, querier }
			},
			Instruction::TransferAsset { assets, beneficiary } => {
				Self::TransferAsset { assets, beneficiary }
			},
			Instruction::TransferReserveAsset { assets, dest, xcm } => {
				Self::TransferReserveAsset { assets, dest, xcm: xcm.try_into()? }
			},
			Instruction::Transact { origin_kind, fallback_max_weight, call } => {
				Self::Transact { origin_kind, fallback_max_weight, call }
			},
			Instruction::HrmpNewChannelOpenRequest { sender, max_message_size, max_capacity } => {
				Self::HrmpNewChannelOpenRequest { sender, max_message_size, max_capacity }
			},
			Instruction::HrmpChannelAccepted { recipient } => {
				Self::HrmpChannelAccepted { recipient }
			},
			Instruction::HrmpChannelClosing { initiator, sender, recipient } => {
				Self::HrmpChannelClosing { initiator, sender, recipient }
			},
			Instruction::ClearOrigin => Self::ClearOrigin,
			Instruction::DescendOrigin(junctions) => Self::DescendOrigin(junctions),
			Instruction::ReportError(query_response_info) => Self::ReportError(query_response_info),
			Instruction::DepositAsset { assets, beneficiary } => {
				Self::DepositAsset { assets, beneficiary }
			},
			Instruction::DepositReserveAsset { assets, dest, xcm } => {
				Self::DepositReserveAsset { assets, dest, xcm: xcm.try_into()? }
			},
			Instruction::ExchangeAsset { give, want, maximal } => {
				Self::ExchangeAsset { give, want, maximal }
			},
			Instruction::InitiateReserveWithdraw { assets, reserve, xcm } => {
				Self::InitiateReserveWithdraw { assets, reserve, xcm: xcm.try_into()? }
			},
			Instruction::InitiateTeleport { assets, dest, xcm } => {
				Self::InitiateTeleport { assets, dest, xcm: xcm.try_into()? }
			},
			Instruction::ReportHolding { response_info, assets } => {
				Self::ReportHolding { response_info, assets }
			},
			Instruction::BuyExecution { fees, weight_limit } => {
				Self::BuyExecution { fees, weight_limit }
			},
			Instruction::RefundSurplus => Self::RefundSurplus,
			Instruction::SetErrorHandler(xcm) => Self::SetErrorHandler(xcm.try_into()?),
			Instruction::SetAppendix(xcm) => Self::SetAppendix(xcm.try_into()?),
			Instruction::ClearError => Self::ClearError,
			Instruction::ClaimAsset { assets, ticket } => Self::ClaimAsset { assets, ticket },
			Instruction::Trap(code) => Self::Trap(code),
			Instruction::SubscribeVersion { query_id, max_response_weight } => {
				Self::SubscribeVersion { query_id, max_response_weight }
			},
			Instruction::UnsubscribeVersion => Self::UnsubscribeVersion,
			Instruction::BurnAsset(assets) => Self::BurnAsset(assets),
			Instruction::ExpectAsset(assets) => Self::ExpectAsset(assets),
			Instruction::ExpectOrigin(location) => Self::ExpectOrigin(location),
			Instruction::ExpectError(_) => Self::ExpectError(None),
			Instruction::ExpectTransactStatus(transact_status) => {
				Self::ExpectTransactStatus(transact_status)
			},
			Instruction::QueryPallet { module_name, response_info } => {
				Self::QueryPallet { module_name, response_info }
			},
			Instruction::ExpectPallet {
				index,
				name,
				module_name,
				crate_major,
				min_crate_minor,
			} => Self::ExpectPallet { index, name, module_name, crate_major, min_crate_minor },
			Instruction::ReportTransactStatus(query_response_info) => {
				Self::ReportTransactStatus(query_response_info)
			},
			Instruction::ClearTransactStatus => Self::ClearTransactStatus,
			Instruction::UniversalOrigin(junction) => Self::UniversalOrigin(junction),
			Instruction::ExportMessage { network, destination, xcm } => {
				Self::ExportMessage { network, destination, xcm: xcm.try_into()? }
			},
			Instruction::LockAsset { asset, unlocker } => Self::LockAsset { asset, unlocker },
			Instruction::UnlockAsset { asset, target } => Self::UnlockAsset { asset, target },
			Instruction::NoteUnlockable { asset, owner } => Self::NoteUnlockable { asset, owner },
			Instruction::RequestUnlock { asset, locker } => Self::RequestUnlock { asset, locker },
			Instruction::SetFeesMode { jit_withdraw } => Self::SetFeesMode { jit_withdraw },
			Instruction::SetTopic(topic) => Self::SetTopic(topic),
			Instruction::ClearTopic => Self::ClearTopic,
			Instruction::AliasOrigin(location) => Self::AliasOrigin(location),
			Instruction::UnpaidExecution { weight_limit, check_origin } => {
				Self::UnpaidExecution { weight_limit, check_origin }
			},
			Instruction::PayFees { asset } => Self::PayFees { asset },
			Instruction::InitiateTransfer {
				destination,
				remote_fees,
				preserve_origin,
				assets,
				remote_xcm,
			} => Self::InitiateTransfer {
				destination,
				remote_fees,
				preserve_origin,
				assets,
				remote_xcm: remote_xcm.try_into()?,
			},
			Instruction::ExecuteWithOrigin { descendant_origin, xcm } => {
				Self::ExecuteWithOrigin { descendant_origin, xcm: xcm.try_into()? }
			},
			Instruction::SetHints { hints } => Self::SetHints { hints },
		})
	}
}

// TODO: Automate Generation
impl<Call, W: XcmWeightInfo<Call>> GetWeight<W> for Instruction<Call> {
	fn weight(&self) -> Weight {
		use Instruction::*;
		match self {
			WithdrawAsset(assets) => W::withdraw_asset(assets),
			ReserveAssetDeposited(assets) => W::reserve_asset_deposited(assets),
			ReceiveTeleportedAsset(assets) => W::receive_teleported_asset(assets),
			QueryResponse { query_id, response, max_weight, querier } =>
				W::query_response(query_id, response, max_weight, querier),
			TransferAsset { assets, beneficiary } => W::transfer_asset(assets, beneficiary),
			TransferReserveAsset { assets, dest, xcm } =>
				W::transfer_reserve_asset(&assets, dest, xcm),
			Transact { origin_kind, fallback_max_weight, call } =>
				W::transact(origin_kind, fallback_max_weight, call),
			HrmpNewChannelOpenRequest { sender, max_message_size, max_capacity } =>
				W::hrmp_new_channel_open_request(sender, max_message_size, max_capacity),
			HrmpChannelAccepted { recipient } => W::hrmp_channel_accepted(recipient),
			HrmpChannelClosing { initiator, sender, recipient } =>
				W::hrmp_channel_closing(initiator, sender, recipient),
			ClearOrigin => W::clear_origin(),
			DescendOrigin(who) => W::descend_origin(who),
			ReportError(response_info) => W::report_error(&response_info),
			DepositAsset { assets, beneficiary } => W::deposit_asset(assets, beneficiary),
			DepositReserveAsset { assets, dest, xcm } =>
				W::deposit_reserve_asset(assets, dest, xcm),
			ExchangeAsset { give, want, maximal } => W::exchange_asset(give, want, maximal),
			InitiateReserveWithdraw { assets, reserve, xcm } =>
				W::initiate_reserve_withdraw(assets, reserve, xcm),
			InitiateTeleport { assets, dest, xcm } => W::initiate_teleport(assets, dest, xcm),
			ReportHolding { response_info, assets } => W::report_holding(&response_info, &assets),
			BuyExecution { fees, weight_limit } => W::buy_execution(fees, weight_limit),
			RefundSurplus => W::refund_surplus(),
			SetErrorHandler(xcm) => W::set_error_handler(xcm),
			SetAppendix(xcm) => W::set_appendix(xcm),
			ClearError => W::clear_error(),
			SetHints { hints } => W::set_hints(hints),
			ClaimAsset { assets, ticket } => W::claim_asset(assets, ticket),
			Trap(code) => W::trap(code),
			SubscribeVersion { query_id, max_response_weight } =>
				W::subscribe_version(query_id, max_response_weight),
			UnsubscribeVersion => W::unsubscribe_version(),
			BurnAsset(assets) => W::burn_asset(assets),
			ExpectAsset(assets) => W::expect_asset(assets),
			ExpectOrigin(origin) => W::expect_origin(origin),
			ExpectError(error) => W::expect_error(error),
			ExpectTransactStatus(transact_status) => W::expect_transact_status(transact_status),
			QueryPallet { module_name, response_info } =>
				W::query_pallet(module_name, response_info),
			ExpectPallet { index, name, module_name, crate_major, min_crate_minor } =>
				W::expect_pallet(index, name, module_name, crate_major, min_crate_minor),
			ReportTransactStatus(response_info) => W::report_transact_status(response_info),
			ClearTransactStatus => W::clear_transact_status(),
			UniversalOrigin(j) => W::universal_origin(j),
			ExportMessage { network, destination, xcm } =>
				W::export_message(network, destination, xcm),
			LockAsset { asset, unlocker } => W::lock_asset(asset, unlocker),
			UnlockAsset { asset, target } => W::unlock_asset(asset, target),
			NoteUnlockable { asset, owner } => W::note_unlockable(asset, owner),
			RequestUnlock { asset, locker } => W::request_unlock(asset, locker),
			SetFeesMode { jit_withdraw } => W::set_fees_mode(jit_withdraw),
			SetTopic(topic) => W::set_topic(topic),
			ClearTopic => W::clear_topic(),
			AliasOrigin(location) => W::alias_origin(location),
			UnpaidExecution { weight_limit, check_origin } =>
				W::unpaid_execution(weight_limit, check_origin),
			PayFees { asset } => W::pay_fees(asset),
			InitiateTransfer { destination, remote_fees, preserve_origin, assets, remote_xcm } =>
				W::initiate_transfer(destination, remote_fees, preserve_origin, assets, remote_xcm),
			ExecuteWithOrigin { descendant_origin, xcm } =>
				W::execute_with_origin(descendant_origin, xcm),
		}
	}
}
