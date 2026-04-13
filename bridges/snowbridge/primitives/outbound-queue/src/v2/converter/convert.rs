// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Converts XCM messages into InboundMessage that can be processed by the Gateway contract

use codec::DecodeAll;
use frame_support::{ensure, BoundedVec};
use snowbridge_core::{AgentIdOf, TokenId, TokenIdOf};

use crate::v2::{
	message::{Command, Message},
	ContractCall,
};

use sp_core::H160;
use sp_runtime::traits::MaybeConvert;
use sp_std::{marker::PhantomData, prelude::*};
use xcm::prelude::*;
use xcm_executor::traits::ConvertLocation;
use XcmConverterError::*;

use super::syntax::{
	ena_asset_matches_snowbridge_shape, network_matches_ethereum, parse_optional_ena_pna,
	parse_remote_fee_section, RemoteFeeParseErr,
};

/// Errors that can be thrown to the pattern matching step.
#[derive(PartialEq, Debug)]
pub enum XcmConverterError {
	UnexpectedEndOfXcm,
	EndOfXcmMessageExpected,
	WithdrawAssetExpected,
	DepositAssetExpected,
	NoReserveAssets,
	FilterDoesNotConsumeAllAssets,
	TooManyAssets,
	ZeroAssetTransfer,
	BeneficiaryResolutionFailed,
	AssetResolutionFailed,
	InvalidFeeAsset,
	SetTopicExpected,
	ReserveAssetDepositedExpected,
	InvalidAsset,
	UnexpectedInstruction,
	TooManyCommands,
	AliasOriginExpected,
	InvalidOrigin,
	TransactDecodeFailed,
	TransactParamsDecodeFailed,
	InvalidContractCallParams,
	FeeAssetResolutionFailed,
	CallContractValueInsufficient,
	NoCommands,
}

/// Minimum forwarded gas for [`ContractCall::V1`]: Ethereum’s intrinsic cost of a simple transfer
/// (21_000); contract calls typically need more, but this rejects obviously non-viable limits.
const MIN_CONTRACT_CALL_GAS: u64 = 21_000;
/// When [`ContractCall::V1`] calldata is non-empty, it must include at least a Solidity function
/// selector (empty calldata remains allowed for agent/value-only patterns used in tests and some
/// flows).
const MIN_NON_EMPTY_CONTRACT_CALL_CALLDATA: usize = 4;

fn decode_contract_call(mut encoded_call: &[u8]) -> Result<ContractCall, XcmConverterError> {
	ContractCall::decode_all(&mut encoded_call).map_err(|_| TransactDecodeFailed)
}

fn ensure_contract_call_v1_params_valid(
	calldata: &[u8],
	gas: u64,
) -> Result<(), XcmConverterError> {
	ensure!(gas >= MIN_CONTRACT_CALL_GAS, InvalidContractCallParams);
	if !calldata.is_empty() {
		ensure!(calldata.len() >= MIN_NON_EMPTY_CONTRACT_CALL_CALLDATA, InvalidContractCallParams);
	}
	Ok(())
}

/// Validates the top-level optional [`Transact`] that follows a v2 outbound [`DepositAsset`].
///
/// This is shared by the exporter pre-simulation path and the converter so both enforce the same
/// minimum gas / calldata rules for [`ContractCall::V1`].
pub(crate) fn ensure_top_level_optional_contract_call_params_valid<Call>(
	instructions: &[Instruction<Call>],
) -> Result<(), XcmConverterError> {
	let mut seen_deposit_asset = false;
	for instruction in instructions {
		match instruction {
			Instruction::DepositAsset { .. } => seen_deposit_asset = true,
			Instruction::Transact { call, .. } if seen_deposit_asset => {
				match decode_contract_call(&call.clone().into_encoded())? {
					ContractCall::V1 { calldata, gas, .. } => {
						ensure_contract_call_v1_params_valid(&calldata, gas)?
					},
				}
				return Ok(());
			},
			Instruction::SetTopic(_) if seen_deposit_asset => return Ok(()),
			_ => {},
		}
	}

	Ok(())
}

macro_rules! match_expression {
	($expression:expr, $(|)? $( $pattern:pat_param )|+ $( if $guard: expr )?, $value:expr $(,)?) => {
		match $expression {
			$( $pattern )|+ $( if $guard )? => Some($value),
			_ => None,
		}
	};
}

pub struct XcmConverter<'a, ConvertAssetId, Call> {
	instructions: &'a [Instruction<Call>],
	pos: usize,
	ethereum_network: NetworkId,
	_marker: PhantomData<ConvertAssetId>,
}
impl<'a, ConvertAssetId, Call> XcmConverter<'a, ConvertAssetId, Call>
where
	ConvertAssetId: MaybeConvert<TokenId, Location>,
{
	pub fn new(message: &'a Xcm<Call>, ethereum_network: NetworkId) -> Self {
		Self {
			instructions: message.inner(),
			pos: 0,
			ethereum_network,
			_marker: Default::default(),
		}
	}

	fn next(&mut self) -> Result<&'a Instruction<Call>, XcmConverterError> {
		let inst = self.instructions.get(self.pos).ok_or(XcmConverterError::UnexpectedEndOfXcm)?;
		self.pos += 1;
		Ok(inst)
	}

	fn peek(&mut self) -> Result<&'a Instruction<Call>, XcmConverterError> {
		self.instructions.get(self.pos).ok_or(XcmConverterError::UnexpectedEndOfXcm)
	}

	/// Extract the fee asset item from PayFees(V5)
	fn extract_remote_fee(&mut self) -> Result<u128, XcmConverterError> {
		let rest = &self.instructions[self.pos..];
		if rest.is_empty() {
			return Err(UnexpectedEndOfXcm);
		}
		let (consumed, fee_amount) = parse_remote_fee_section(rest).map_err(|e| match e {
			RemoteFeeParseErr::ExpectedWithdrawAsset => WithdrawAssetExpected,
			RemoteFeeParseErr::UnexpectedEndAfterWithdraw => UnexpectedEndOfXcm,
			RemoteFeeParseErr::AssetResolutionFailed => AssetResolutionFailed,
			RemoteFeeParseErr::InvalidFeeAsset => InvalidFeeAsset,
		})?;
		self.pos += consumed;
		Ok(fee_amount)
	}

	/// Extract ethereum native assets
	fn extract_ethereum_native_assets(
		&mut self,
		enas: &Assets,
		deposit_assets: &AssetFilter,
		recipient: H160,
	) -> Result<Vec<Command>, XcmConverterError> {
		let mut commands: Vec<Command> = Vec::new();
		for ena in enas.clone().into_inner().into_iter() {
			if !deposit_assets.matches(&ena) {
				return Err(FilterDoesNotConsumeAllAssets);
			}

			if !ena_asset_matches_snowbridge_shape(&ena, self.ethereum_network) {
				return Err(AssetResolutionFailed);
			}

			let (token, amount) = match ena {
				Asset { id: AssetId(inner_location), fun: Fungible(amount) } => {
					match inner_location.unpack() {
						(0, [AccountKey20 { network, key }])
							if network_matches_ethereum(network, self.ethereum_network) =>
						{
							Ok((H160(*key), amount))
						},
						(0, []) => Ok((H160([0; 20]), amount)),
						_ => Err(AssetResolutionFailed),
					}
				},
				_ => Err(AssetResolutionFailed),
			}?;

			ensure!(amount > 0, ZeroAssetTransfer);

			commands.push(Command::UnlockNativeToken { token, recipient, amount });
		}
		Ok(commands)
	}

	/// Extract polkadot native assets
	fn extract_polkadot_native_assets(
		&mut self,
		pnas: &Assets,
		deposit_assets: &AssetFilter,
		recipient: H160,
	) -> Result<Vec<Command>, XcmConverterError> {
		let mut commands: Vec<Command> = Vec::new();
		ensure!(pnas.len() > 0, NoReserveAssets);
		for pna in pnas.clone().into_inner().into_iter() {
			if !deposit_assets.matches(&pna) {
				return Err(FilterDoesNotConsumeAllAssets);
			}

			let Asset { id: AssetId(asset_id), fun: Fungible(amount) } = pna else {
				return Err(AssetResolutionFailed);
			};

			ensure!(amount > 0, ZeroAssetTransfer);

			let token_id = TokenIdOf::convert_location(&asset_id).ok_or(InvalidAsset)?;
			let expected_asset_id = ConvertAssetId::maybe_convert(token_id).ok_or(InvalidAsset)?;
			ensure!(asset_id == expected_asset_id, InvalidAsset);

			commands.push(Command::MintForeignToken { token_id, recipient, amount });
		}
		Ok(commands)
	}

	/// Convert the XCM into an outbound message which can be dispatched to
	/// the Gateway contract on Ethereum
	///
	/// Assets being transferred can either be Polkadot-native assets (PNA)
	/// or Ethereum-native assets (ENA).
	///
	/// The XCM is evaluated in Ethereum context.
	///
	/// Expected Input Syntax:
	/// ```ignore
	/// WithdrawAsset(ETH)
	/// PayFees(ETH)
	/// ReserveAssetDeposited(PNA) | WithdrawAsset(ENA)
	/// AliasOrigin(Origin)
	/// DepositAsset(Asset)
	/// Transact() [OPTIONAL]
	/// SetTopic(Topic)
	/// ```
	///
	/// Structural validation for early rejection (e.g. Bridge Hub Ethereum XCM simulation barrier):
	/// [`super::shape::snowbridge_v2_outbound_xcm_shape`].
	/// Notes:
	/// a. Fee asset will be checked and currently only Ether is allowed
	/// b. For a specific transfer, either `ReserveAssetDeposited` or `WithdrawAsset` should be
	/// 	present
	/// c. `ReserveAssetDeposited` and `WithdrawAsset` can also be present in any order within the
	/// 	same message
	/// d. Currently, teleport asset is not allowed, transfer types other than
	/// 	above will cause the conversion to fail
	/// e. Currently, `AliasOrigin` is always required, can distinguish the V2 process from V1.
	/// 	it's required also for dispatching transact from that specific origin.
	/// f. SetTopic is required for tracing the message all the way along.
	pub fn convert(&mut self) -> Result<Message, XcmConverterError> {
		let fee_amount = self.extract_remote_fee()?;

		let (next_pos, enas, pnas) = parse_optional_ena_pna(self.instructions, self.pos);
		self.pos = next_pos;

		let origin_location = match_expression!(self.next()?, AliasOrigin(origin), origin)
			.ok_or(AliasOriginExpected)?;
		let origin = AgentIdOf::convert_location(origin_location).ok_or(InvalidOrigin)?;

		let (deposit_assets, beneficiary) = match_expression!(
			self.next()?,
			DepositAsset { assets, beneficiary },
			(assets, beneficiary)
		)
		.ok_or(DepositAssetExpected)?;

		let recipient = match_expression!(
			beneficiary.unpack(),
			(0, [AccountKey20 { network, key }])
				if network_matches_ethereum(network, self.ethereum_network),
			H160(*key)
		)
		.ok_or(BeneficiaryResolutionFailed)?;

		let mut commands: Vec<Command> = Vec::new();

		if let Some(enas) = enas {
			commands.append(&mut self.extract_ethereum_native_assets(
				enas,
				deposit_assets,
				recipient,
			)?);
		}

		if let Some(pnas) = pnas {
			commands.append(&mut self.extract_polkadot_native_assets(
				pnas,
				deposit_assets,
				recipient,
			)?);
		}

		let transact_call = match_expression!(self.peek()?, Transact { call, .. }, call);
		if let Some(transact_call) = transact_call {
			let _ = self.next();
			let transact = decode_contract_call(&transact_call.clone().into_encoded())?;
			match transact {
				ContractCall::V1 { target, calldata, gas, value } => {
					ensure_contract_call_v1_params_valid(&calldata, gas)?;
					commands.push(Command::CallContract {
						target: target.into(),
						calldata,
						gas,
						value,
					});
				},
			}
		}

		ensure!(commands.len() > 0, NoCommands);

		let topic_id = match_expression!(self.next()?, SetTopic(id), id).ok_or(SetTopicExpected)?;

		let message = Message {
			id: (*topic_id).into(),
			origin,
			fee: fee_amount,
			commands: BoundedVec::try_from(commands).map_err(|_| TooManyCommands)?,
		};

		if self.next().is_ok() {
			return Err(EndOfXcmMessageExpected);
		}

		Ok(message)
	}
}
