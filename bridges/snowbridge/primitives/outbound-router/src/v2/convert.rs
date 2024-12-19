// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Converts XCM messages into InboundMessage that can be processed by the Gateway contract

use codec::DecodeAll;
use core::slice::Iter;
use frame_support::{ensure, traits::Get, BoundedVec};
use snowbridge_core::{AgentIdOf, TokenId, TokenIdOf};
use snowbridge_outbound_primitives::{
	v2::{Command, Message},
	TransactInfo,
};
use sp_core::H160;
use sp_runtime::traits::MaybeEquivalence;
use sp_std::{iter::Peekable, marker::PhantomData, prelude::*};
use xcm::prelude::*;
use xcm_executor::traits::ConvertLocation;

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
	FeeAssetResolutionFailed,
	CallContractValueInsufficient,
}

macro_rules! match_expression {
	($expression:expr, $(|)? $( $pattern:pat_param )|+ $( if $guard: expr )?, $value:expr $(,)?) => {
		match $expression {
			$( $pattern )|+ $( if $guard )? => Some($value),
			_ => None,
		}
	};
}

pub struct XcmConverter<'a, ConvertAssetId, WETHAddress, Call> {
	iter: Peekable<Iter<'a, Instruction<Call>>>,
	ethereum_network: NetworkId,
	_marker: PhantomData<(ConvertAssetId, WETHAddress)>,
}
impl<'a, ConvertAssetId, WETHAddress, Call> XcmConverter<'a, ConvertAssetId, WETHAddress, Call>
where
	ConvertAssetId: MaybeEquivalence<TokenId, Location>,
	WETHAddress: Get<H160>,
{
	pub fn new(message: &'a Xcm<Call>, ethereum_network: NetworkId) -> Self {
		Self {
			iter: message.inner().iter().peekable(),
			ethereum_network,
			_marker: Default::default(),
		}
	}

	pub fn convert(&mut self) -> Result<Message, XcmConverterError> {
		let result = self.to_ethereum_message()?;
		Ok(result)
	}

	fn next(&mut self) -> Result<&'a Instruction<Call>, XcmConverterError> {
		self.iter.next().ok_or(XcmConverterError::UnexpectedEndOfXcm)
	}

	fn peek(&mut self) -> Result<&&'a Instruction<Call>, XcmConverterError> {
		self.iter.peek().ok_or(XcmConverterError::UnexpectedEndOfXcm)
	}

	fn network_matches(&self, network: &Option<NetworkId>) -> bool {
		if let Some(network) = network {
			*network == self.ethereum_network
		} else {
			true
		}
	}

	/// Extract the fee asset item from PayFees(V5)
	fn extract_remote_fee(&mut self) -> Result<u128, XcmConverterError> {
		use XcmConverterError::*;
		let _ = match_expression!(self.next()?, WithdrawAsset(fee), fee)
			.ok_or(WithdrawAssetExpected)?;
		let fee_asset =
			match_expression!(self.next()?, PayFees { asset: fee }, fee).ok_or(InvalidFeeAsset)?;
		let (fee_asset_id, fee_amount) = match fee_asset {
			Asset { id: asset_id, fun: Fungible(amount) } => Some((asset_id, *amount)),
			_ => None,
		}
		.ok_or(AssetResolutionFailed)?;
		let weth_address = match_expression!(
			fee_asset_id.0.unpack(),
			(0, [AccountKey20 { network, key }])
				if self.network_matches(network),
			H160(*key)
		)
		.ok_or(FeeAssetResolutionFailed)?;
		ensure!(weth_address == WETHAddress::get(), InvalidFeeAsset);
		Ok(fee_amount)
	}

	/// Convert the xcm for into the Message which will be executed
	/// on Ethereum Gateway contract, we expect an input of the form:
	/// # WithdrawAsset(WETH)
	/// # PayFees(WETH)
	/// # ReserveAssetDeposited(PNA) | WithdrawAsset(ENA)
	/// # AliasOrigin(Origin)
	/// # DepositAsset(PNA|ENA)
	/// # Transact() ---Optional
	/// # SetTopic
	fn to_ethereum_message(&mut self) -> Result<Message, XcmConverterError> {
		use XcmConverterError::*;

		// Get fee amount
		let fee_amount = self.extract_remote_fee()?;

		// Get ENA reserve asset from WithdrawAsset.
		let mut enas =
			match_expression!(self.peek(), Ok(WithdrawAsset(reserve_assets)), reserve_assets);
		if enas.is_some() {
			let _ = self.next();
		}

		// Get PNA reserve asset from ReserveAssetDeposited
		let pnas = match_expression!(
			self.peek(),
			Ok(ReserveAssetDeposited(reserve_assets)),
			reserve_assets
		);
		if pnas.is_some() {
			let _ = self.next();
		}

		// Try to get ENA again if it is after PNA
		if enas.is_none() {
			enas =
				match_expression!(self.peek(), Ok(WithdrawAsset(reserve_assets)), reserve_assets);
			if enas.is_some() {
				let _ = self.next();
			}
		}
		// Check AliasOrigin.
		let origin_location = match_expression!(self.next()?, AliasOrigin(origin), origin)
			.ok_or(AliasOriginExpected)?;
		let origin = AgentIdOf::convert_location(origin_location).ok_or(InvalidOrigin)?;

		let (deposit_assets, beneficiary) = match_expression!(
			self.next()?,
			DepositAsset { assets, beneficiary },
			(assets, beneficiary)
		)
		.ok_or(DepositAssetExpected)?;

		// assert that the beneficiary is AccountKey20.
		let recipient = match_expression!(
			beneficiary.unpack(),
			(0, [AccountKey20 { network, key }])
				if self.network_matches(network),
			H160(*key)
		)
		.ok_or(BeneficiaryResolutionFailed)?;

		// Make sure there are reserved assets.
		if enas.is_none() && pnas.is_none() {
			return Err(NoReserveAssets)
		}

		let mut commands: Vec<Command> = Vec::new();

		// ENA transfer commands
		if let Some(enas) = enas {
			for ena in enas.clone().inner().iter() {
				// Check the the deposit asset filter matches what was reserved.
				if !deposit_assets.matches(ena) {
					return Err(FilterDoesNotConsumeAllAssets)
				}

				// only fungible asset is allowed
				let (token, amount) = match ena {
					Asset { id: AssetId(inner_location), fun: Fungible(amount) } =>
						match inner_location.unpack() {
							(0, [AccountKey20 { network, key }])
								if self.network_matches(network) =>
								Some((H160(*key), *amount)),
							_ => None,
						},
					_ => None,
				}
				.ok_or(AssetResolutionFailed)?;

				// transfer amount must be greater than 0.
				ensure!(amount > 0, ZeroAssetTransfer);

				commands.push(Command::UnlockNativeToken { token, recipient, amount });
			}
		}

		// PNA transfer commands
		if let Some(pnas) = pnas {
			ensure!(pnas.len() > 0, NoReserveAssets);
			for pna in pnas.clone().inner().iter() {
				// Check the the deposit asset filter matches what was reserved.
				if !deposit_assets.matches(pna) {
					return Err(FilterDoesNotConsumeAllAssets)
				}

				// Only fungible is allowed
				let (asset_id, amount) = match pna {
					Asset { id: AssetId(inner_location), fun: Fungible(amount) } =>
						Some((inner_location.clone(), *amount)),
					_ => None,
				}
				.ok_or(AssetResolutionFailed)?;

				// transfer amount must be greater than 0.
				ensure!(amount > 0, ZeroAssetTransfer);

				// Ensure PNA already registered
				let token_id = TokenIdOf::convert_location(&asset_id).ok_or(InvalidAsset)?;
				let expected_asset_id = ConvertAssetId::convert(&token_id).ok_or(InvalidAsset)?;
				ensure!(asset_id == expected_asset_id, InvalidAsset);

				commands.push(Command::MintForeignToken { token_id, recipient, amount });
			}
		}

		// Transact commands
		let transact_call = match_expression!(self.peek(), Ok(Transact { call, .. }), call);
		if let Some(transact_call) = transact_call {
			let _ = self.next();
			let transact =
				TransactInfo::decode_all(&mut transact_call.clone().into_encoded().as_slice())
					.map_err(|_| TransactDecodeFailed)?;
			commands.push(Command::CallContract {
				target: transact.target,
				data: transact.data,
				gas_limit: transact.gas_limit,
				value: transact.value,
			});
		}

		// ensure SetTopic exists
		let topic_id = match_expression!(self.next()?, SetTopic(id), id).ok_or(SetTopicExpected)?;

		let message = Message {
			id: (*topic_id).into(),
			origin_location: origin_location.clone(),
			origin,
			fee: fee_amount,
			commands: BoundedVec::try_from(commands).map_err(|_| TooManyCommands)?,
		};

		// All xcm instructions must be consumed before exit.
		if self.next().is_ok() {
			return Err(EndOfXcmMessageExpected)
		}

		Ok(message)
	}
}
