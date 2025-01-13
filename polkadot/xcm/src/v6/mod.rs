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
use super::v5::{Instruction as OldInstruction, Xcm as OldXcm, MAX_INSTRUCTIONS_TO_DECODE};
use crate::v5::Weight;
use alloc::{vec, vec::Vec};
use codec::{
	self, decode_vec_with_len, Compact, Decode, Encode, Error as CodecError, Input as CodecInput,
};
use core::result;
use educe::Educe;
use scale_info::TypeInfo;

pub mod instruction;
pub mod instructions;

use crate::impl_xcm_instruction;
pub use instructions::*;

pub const VERSION: super::Version = 6;

#[derive(Educe, Default, Encode, TypeInfo)]
#[educe(Clone, Eq, PartialEq, Debug)]
#[codec(encode_bound())]
#[codec(decode_bound())]
#[scale_info(bounds(), skip_type_params(Call))]
pub struct Xcm<Call>(pub Vec<Instruction<Call>>);

environmental::environmental!(instructions_count: u8);

impl<Call> Decode for Xcm<Call> {
	fn decode<I: CodecInput>(input: &mut I) -> core::result::Result<Self, CodecError> {
		instructions_count::using_once(&mut 0, || {
			let number_of_instructions: u32 = <Compact<u32>>::decode(input)?.into();
			instructions_count::with(|count| {
				*count = count.saturating_add(number_of_instructions as u8);
				if *count > MAX_INSTRUCTIONS_TO_DECODE {
					return Err(CodecError::from("Max instructions exceeded"));
				}
				Ok(())
			})
			.expect("Called in `using` context and thus can not return `None`; qed")?;
			let decoded_instructions = decode_vec_with_len(input, number_of_instructions as usize)?;
			Ok(Self(decoded_instructions))
		})
	}
}

impl<Call> Xcm<Call> {
	/// Create an empty instance.
	pub fn new() -> Self {
		Self(vec![])
	}

	/// Return `true` if no instructions are held in `self`.
	pub fn is_empty(&self) -> bool {
		self.0.is_empty()
	}

	/// Return the number of instructions held in `self`.
	pub fn len(&self) -> usize {
		self.0.len()
	}

	/// Return a reference to the inner value.
	pub fn inner(&self) -> &[Instruction<Call>] {
		&self.0
	}

	/// Return a mutable reference to the inner value.
	pub fn inner_mut(&mut self) -> &mut Vec<Instruction<Call>> {
		&mut self.0
	}

	/// Consume and return the inner value.
	pub fn into_inner(self) -> Vec<Instruction<Call>> {
		self.0
	}

	/// Return an iterator over references to the items.
	pub fn iter(&self) -> impl Iterator<Item = &Instruction<Call>> {
		self.0.iter()
	}

	/// Return an iterator over mutable references to the items.
	pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Instruction<Call>> {
		self.0.iter_mut()
	}

	/// Consume and return an iterator over the items.
	pub fn into_iter(self) -> impl Iterator<Item = Instruction<Call>> {
		self.0.into_iter()
	}

	/// Consume and either return `self` if it contains some instructions, or if it's empty, then
	/// instead return the result of `f`.
	pub fn or_else(self, f: impl FnOnce() -> Self) -> Self {
		if self.0.is_empty() {
			f()
		} else {
			self
		}
	}

	/// Return the first instruction, if any.
	pub fn first(&self) -> Option<&Instruction<Call>> {
		self.0.first()
	}

	/// Return the last instruction, if any.
	pub fn last(&self) -> Option<&Instruction<Call>> {
		self.0.last()
	}

	/// Return the only instruction, contained in `Self`, iff only one exists (`None` otherwise).
	pub fn only(&self) -> Option<&Instruction<Call>> {
		if self.0.len() == 1 {
			self.0.first()
		} else {
			None
		}
	}

	/// Return the only instruction, contained in `Self`, iff only one exists (returns `self`
	/// otherwise).
	pub fn into_only(mut self) -> core::result::Result<Instruction<Call>, Self> {
		if self.0.len() == 1 {
			self.0.pop().ok_or(self)
		} else {
			Err(self)
		}
	}
}

impl<Call> From<Vec<Instruction<Call>>> for Xcm<Call> {
	fn from(c: Vec<Instruction<Call>>) -> Self {
		Self(c)
	}
}

impl<Call> From<Xcm<Call>> for Vec<Instruction<Call>> {
	fn from(c: Xcm<Call>) -> Self {
		c.0
	}
}

/// A prelude for importing all types typically used when interacting with XCM messages.
pub mod prelude {
	mod contents {
		pub use super::super::VERSION as XCM_VERSION;
		pub use crate::v5::prelude::{
			send_xcm, validate_send, Ancestor, AncestorThen, Asset,
			AssetFilter::{self, *},
			AssetId,
			AssetInstance::{self, *},
			Assets, BodyId, BodyPart, ExecuteXcm,
			Fungibility::{self, *},
			Hint::{self, *},
			HintNumVariants,
			Instruction::*,
			InteriorLocation,
			Junction::{self, *},
			Junctions::{self, Here},
			Location, MaybeErrorCode,
			NetworkId::{self, *},
			OriginKind, Outcome, PalletInfo, Parent, ParentThen, PreparedMessage, QueryId,
			QueryResponseInfo, Reanchorable, Response, SendError, SendResult, SendXcm, Weight,
			WeightLimit::{self, *},
			WildAsset::{self, *},
			WildFungibility::{self, Fungible as WildFungible, NonFungible as WildNonFungible},
			XcmContext, XcmError, XcmHash, XcmResult, XcmWeightInfo,
		};
	}
	pub use super::{Instruction, Xcm};
	pub use contents::*;
	pub mod opaque {
		pub use super::{
			super::opaque::{Instruction, Xcm},
			contents::*,
		};
	}
}

pub trait IntoInstruction<Call> {
	fn into_instruction(self) -> Instruction<Call>;
}

impl_xcm_instruction! {
	/// Cross-Consensus Message: A message from one consensus system to another.
	///
	/// Consensus systems that may send and receive messages include blockchains and smart contracts.
	///
	/// All messages are delivered from a known *origin*, expressed as a `Location`.
	///
	/// This is the inner XCM format and is version-sensitive. Messages are typically passed using the
	/// outer XCM format, known as `VersionedXcm`.
	#[derive(
		Educe,
		Encode,
		Decode,
		TypeInfo,
		xcm_procedural::XcmWeightInfoTrait,
		xcm_procedural::Builder,
	)]
	#[educe(Clone, Eq, PartialEq, Debug)]
	#[codec(encode_bound())]
	#[codec(decode_bound())]
	#[scale_info(bounds(), skip_type_params(Call))]

	pub enum Instruction<Call> {
		#[builder(loads_holding)] // TODO: move builder attribute to the instruction struct
		WithdrawAsset,
		#[builder(loads_holding)]
		ReserveAssetDeposited,
		#[builder(loads_holding)]
		ReceiveTeleportedAsset,
		QueryResponse,
		TransferAsset,
		TransferReserveAsset,
		Transact<Call>,
		HrmpNewChannelOpenRequest,
		HrmpChannelAccepted,
		HrmpChannelClosing,
		ClearOrigin,
		DescendOrigin,
		ReportError,
		DepositAsset,
		DepositReserveAsset,
		ExchangeAsset,
		InitiateReserveWithdraw,
		InitiateTeleport,
		ReportHolding,
		#[builder(pays_fees)]
		BuyExecution,
		RefundSurplus,
		SetErrorHandler<Call>,
		SetAppendix<Call>,
		ClearError,
		#[builder(loads_holding)]
		ClaimAsset,
		Trap,
		SubscribeVersion,
		UnsubscribeVersion,
		BurnAsset,
		ExpectAsset,
		ExpectOrigin,
		ExpectError,
		ExpectTransactStatus,
		QueryPallet,
		ExpectPallet,
		ReportTransactStatus,
		ClearTransactStatus,
		UniversalOrigin,
		ExportMessage,
		LockAsset,
		UnlockAsset,
		NoteUnlockable,
		RequestUnlock,
		SetFeesMode,
		SetTopic,
		ClearTopic,
		AliasOrigin,
		UnpaidExecution,
		#[builder(pays_fees)]
		PayFees,
		InitiateTransfer,
		ExecuteWithOrigin<Call>,
		SetHints,
	}
}

impl<Call> Xcm<Call> {
	pub fn into<C>(self) -> Xcm<C> {
		Xcm::from(self)
	}
	pub fn from<C>(xcm: Xcm<C>) -> Self {
		Self(xcm.0.into_iter().map(Instruction::<Call>::from).collect())
	}
}

// // TODO: Automate Generation
// impl<Call, W: XcmWeightInfo<Call>> GetWeight<W> for Instruction<Call> {
// 	fn weight(&self) -> Weight {
// 		use Instruction::*;
// 		match self {
// 			WithdrawAsset(assets) => W::withdraw_asset(assets),
// 			ReserveAssetDeposited(assets) => W::reserve_asset_deposited(assets),
// 			ReceiveTeleportedAsset(assets) => W::receive_teleported_asset(assets),
// 			QueryResponse { query_id, response, max_weight, querier } =>
// 				W::query_response(query_id, response, max_weight, querier),
// 			TransferAsset { assets, beneficiary } => W::transfer_asset(assets, beneficiary),
// 			TransferReserveAsset { assets, dest, xcm } =>
// 				W::transfer_reserve_asset(&assets, dest, xcm),
// 			Transact { origin_kind, fallback_max_weight, call } =>
// 				W::transact(origin_kind, fallback_max_weight, call),
// 			HrmpNewChannelOpenRequest { sender, max_message_size, max_capacity } =>
// 				W::hrmp_new_channel_open_request(sender, max_message_size, max_capacity),
// 			HrmpChannelAccepted { recipient } => W::hrmp_channel_accepted(recipient),
// 			HrmpChannelClosing { initiator, sender, recipient } =>
// 				W::hrmp_channel_closing(initiator, sender, recipient),
// 			ClearOrigin => W::clear_origin(),
// 			DescendOrigin(who) => W::descend_origin(who),
// 			ReportError(response_info) => W::report_error(&response_info),
// 			DepositAsset { assets, beneficiary } => W::deposit_asset(assets, beneficiary),
// 			DepositReserveAsset { assets, dest, xcm } =>
// 				W::deposit_reserve_asset(assets, dest, xcm),
// 			ExchangeAsset { give, want, maximal } => W::exchange_asset(give, want, maximal),
// 			InitiateReserveWithdraw { assets, reserve, xcm } =>
// 				W::initiate_reserve_withdraw(assets, reserve, xcm),
// 			InitiateTeleport { assets, dest, xcm } => W::initiate_teleport(assets, dest, xcm),
// 			ReportHolding { response_info, assets } => W::report_holding(&response_info, &assets),
// 			BuyExecution { fees, weight_limit } => W::buy_execution(fees, weight_limit),
// 			RefundSurplus => W::refund_surplus(),
// 			SetErrorHandler(xcm) => W::set_error_handler(xcm),
// 			SetAppendix(xcm) => W::set_appendix(xcm),
// 			ClearError => W::clear_error(),
// 			SetHints { hints } => W::set_hints(hints),
// 			ClaimAsset { assets, ticket } => W::claim_asset(assets, ticket),
// 			Trap(code) => W::trap(code),
// 			SubscribeVersion { query_id, max_response_weight } =>
// 				W::subscribe_version(query_id, max_response_weight),
// 			UnsubscribeVersion => W::unsubscribe_version(),
// 			BurnAsset(assets) => W::burn_asset(assets),
// 			ExpectAsset(assets) => W::expect_asset(assets),
// 			ExpectOrigin(origin) => W::expect_origin(origin),
// 			ExpectError(error) => W::expect_error(error),
// 			ExpectTransactStatus(transact_status) => W::expect_transact_status(transact_status),
// 			QueryPallet { module_name, response_info } =>
// 				W::query_pallet(module_name, response_info),
// 			ExpectPallet { index, name, module_name, crate_major, min_crate_minor } =>
// 				W::expect_pallet(index, name, module_name, crate_major, min_crate_minor),
// 			ReportTransactStatus(response_info) => W::report_transact_status(response_info),
// 			ClearTransactStatus => W::clear_transact_status(),
// 			UniversalOrigin(j) => W::universal_origin(j),
// 			ExportMessage { network, destination, xcm } =>
// 				W::export_message(network, destination, xcm),
// 			LockAsset { asset, unlocker } => W::lock_asset(asset, unlocker),
// 			UnlockAsset { asset, target } => W::unlock_asset(asset, target),
// 			NoteUnlockable { asset, owner } => W::note_unlockable(asset, owner),
// 			RequestUnlock { asset, locker } => W::request_unlock(asset, locker),
// 			SetFeesMode { jit_withdraw } => W::set_fees_mode(jit_withdraw),
// 			SetTopic(topic) => W::set_topic(topic),
// 			ClearTopic => W::clear_topic(),
// 			AliasOrigin(location) => W::alias_origin(location),
// 			UnpaidExecution { weight_limit, check_origin } =>
// 				W::unpaid_execution(weight_limit, check_origin),
// 			PayFees { asset } => W::pay_fees(asset),
// 			InitiateTransfer { destination, remote_fees, preserve_origin, assets, remote_xcm } =>
// 				W::initiate_transfer(destination, remote_fees, preserve_origin, assets, remote_xcm),
// 			ExecuteWithOrigin { descendant_origin, xcm } =>
// 				W::execute_with_origin(descendant_origin, xcm),
// 		}
// 	}
// }

pub mod opaque {
	/// The basic concrete type of `Xcm`, which doesn't make any assumptions about the
	/// format of a call other than it is pre-encoded.
	pub type Xcm = super::Xcm<()>;

	/// The basic concrete type of `Instruction`, which doesn't make any assumptions about the
	/// format of a call other than it is pre-encoded.
	pub type Instruction = super::Instruction<()>;
}

// Convert from a v5 XCM to a v6 XCM
impl<Call> TryFrom<OldXcm<Call>> for Xcm<Call> {
	type Error = ();
	fn try_from(old_xcm: OldXcm<Call>) -> result::Result<Self, Self::Error> {
		Ok(Xcm(old_xcm.0.into_iter().map(TryInto::try_into).collect::<result::Result<_, _>>()?))
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
				TransferReserveAsset { assets, dest, xcm }.into_instruction()
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
				DepositReserveAsset { assets, dest, xcm }.into_instruction()
			},
			OldInstruction::ExchangeAsset { give, want, maximal } => {
				ExchangeAsset { give, want, maximal }.into_instruction()
			},
			OldInstruction::InitiateReserveWithdraw { assets, reserve, xcm } => {
				InitiateReserveWithdraw { assets, reserve, xcm }.into_instruction()
			},
			OldInstruction::InitiateTeleport { assets, dest, xcm } => {
				InitiateTeleport { assets, dest, xcm }.into_instruction()
			},
			OldInstruction::ReportHolding { response_info, assets } => {
				ReportHolding { response_info, assets }.into_instruction()
			},
			OldInstruction::BuyExecution { fees, weight_limit } => {
				BuyExecution { fees, weight_limit }.into_instruction()
			},
			OldInstruction::RefundSurplus => RefundSurplus.into_instruction(),
			OldInstruction::SetErrorHandler(xcm) => SetErrorHandler(xcm).into_instruction(),
			OldInstruction::SetAppendix(xcm) => SetAppendix(xcm).into_instruction(),
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
				ExportMessage { network, destination, xcm }.into_instruction()
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
			} => InitiateTransfer { destination, remote_fees, preserve_origin, assets, remote_xcm }
				.into_instruction(),
			OldInstruction::ExecuteWithOrigin { descendant_origin, xcm } => {
				ExecuteWithOrigin { descendant_origin, xcm }.into_instruction()
			},
			OldInstruction::SetHints { hints } => SetHints { hints }.into_instruction(),
		})
	}
}
