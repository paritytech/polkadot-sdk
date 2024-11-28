// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Converts XCM messages into InboundMessage that can be processed by the Gateway contract

use codec::DecodeAll;
use core::slice::Iter;
use frame_support::{ensure, traits::Get, BoundedVec};
use snowbridge_core::{
	outbound::v2::{Command, Message},
	transact::{CallContractParams, RegisterTokenParams, TransactInfo, TransactKind::*},
	TokenId, TokenIdOf, TokenIdOf as LocationIdOf,
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
		let enas =
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
		// Check AliasOrigin.
		let origin_location = match_expression!(self.next()?, AliasOrigin(origin), origin)
			.ok_or(AliasOriginExpected)?;
		let origin = LocationIdOf::convert_location(origin_location).ok_or(InvalidOrigin)?;

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
		let mut weth_amount = 0;

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

				if token == WETHAddress::get() {
					weth_amount = amount;
				}

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
			let message =
				TransactInfo::decode_all(&mut transact_call.clone().into_encoded().as_slice())
					.map_err(|_| TransactDecodeFailed)?;
			match message.kind {
				RegisterAgent => commands.push(Command::CreateAgent {}),
				RegisterToken => {
					let params = RegisterTokenParams::decode_all(&mut message.params.as_slice())
						.map_err(|_| TransactParamsDecodeFailed)?;
					let token_id =
						TokenIdOf::convert_location(&params.location).ok_or(InvalidAsset)?;
					commands.push(Command::RegisterForeignToken {
						token_id,
						name: params.metadata.name.into_inner(),
						symbol: params.metadata.symbol.into_inner(),
						decimals: params.metadata.decimals,
					});
				},
				CallContract => {
					let params = CallContractParams::decode_all(&mut message.params.as_slice())
						.map_err(|_| TransactParamsDecodeFailed)?;
					if params.value > 0 {
						ensure!(weth_amount > params.value, CallContractValueInsufficient);
					}
					commands.push(Command::CallContract {
						target: params.target,
						data: params.data,
						gas_limit: params.gas_limit,
						value: params.value,
					});
				},
			}
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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::outbound::v2::tests::{
		BridgedNetwork, MockTokenIdConvert, NonBridgedNetwork, WETHAddress,
	};
	use hex_literal::hex;
	use snowbridge_core::AgentIdOf;
	use xcm::latest::WESTEND_GENESIS_HASH;

	#[test]
	fn xcm_converter_convert_success() {
		let network = BridgedNetwork::get();

		let token_address: [u8; 20] = hex!("1000000000000000000000000000000000000000");
		let beneficiary_address: [u8; 20] = hex!("2000000000000000000000000000000000000000");

		let assets: Assets = vec![Asset {
			id: AssetId([AccountKey20 { network: None, key: token_address }].into()),
			fun: Fungible(1000),
		}]
		.into();
		let filter: AssetFilter = assets.clone().into();

		let fee_asset: Asset = Asset {
			id: AssetId([AccountKey20 { network: None, key: WETHAddress::get().0 }].into()),
			fun: Fungible(1000),
		}
		.into();

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
			PayFees { asset: fee_asset },
			WithdrawAsset(assets.clone()),
			AliasOrigin(Location::new(1, [GlobalConsensus(Polkadot), Parachain(1000)])),
			DepositAsset {
				assets: filter,
				beneficiary: AccountKey20 { network: None, key: beneficiary_address }.into(),
			},
			SetTopic([0; 32]),
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, WETHAddress, ()>::new(&message, network);
		let result = converter.convert();
		assert!(result.is_ok());
	}

	#[test]
	fn xcm_converter_convert_with_wildcard_all_asset_filter_succeeds() {
		let network = BridgedNetwork::get();

		let token_address: [u8; 20] = hex!("1000000000000000000000000000000000000000");
		let beneficiary_address: [u8; 20] = hex!("2000000000000000000000000000000000000000");

		let assets: Assets = vec![Asset {
			id: AssetId([AccountKey20 { network: None, key: token_address }].into()),
			fun: Fungible(1000),
		}]
		.into();
		let filter: AssetFilter = Wild(All);
		let fee_asset: Asset = Asset {
			id: AssetId([AccountKey20 { network: None, key: WETHAddress::get().0 }].into()),
			fun: Fungible(1000),
		}
		.into();

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
			PayFees { asset: fee_asset },
			WithdrawAsset(assets.clone()),
			AliasOrigin(Location::new(1, [GlobalConsensus(Polkadot), Parachain(1000)])),
			DepositAsset {
				assets: filter,
				beneficiary: AccountKey20 { network: None, key: beneficiary_address }.into(),
			},
			SetTopic([0; 32]),
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, WETHAddress, ()>::new(&message, network);
		let result = converter.convert();
		assert_eq!(result.is_ok(), true);
	}

	#[test]
	fn xcm_converter_convert_without_set_topic_yields_set_topic_expected() {
		let network = BridgedNetwork::get();

		let token_address: [u8; 20] = hex!("1000000000000000000000000000000000000000");
		let beneficiary_address: [u8; 20] = hex!("2000000000000000000000000000000000000000");

		let assets: Assets = vec![Asset {
			id: AssetId([AccountKey20 { network: None, key: token_address }].into()),
			fun: Fungible(1000),
		}]
		.into();
		let filter: AssetFilter = assets.clone().into();
		let fee_asset: Asset = Asset {
			id: AssetId([AccountKey20 { network: None, key: WETHAddress::get().0 }].into()),
			fun: Fungible(1000),
		}
		.into();

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
			PayFees { asset: fee_asset },
			WithdrawAsset(assets.clone()),
			AliasOrigin(Location::new(1, [GlobalConsensus(Polkadot), Parachain(1000)])),
			DepositAsset {
				assets: filter,
				beneficiary: AccountKey20 { network: None, key: beneficiary_address }.into(),
			},
			ClearTopic,
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, WETHAddress, ()>::new(&message, network);
		let result = converter.convert();
		assert_eq!(result.err(), Some(XcmConverterError::SetTopicExpected));
	}

	#[test]
	fn xcm_converter_convert_with_partial_message_yields_invalid_fee_asset() {
		let network = BridgedNetwork::get();

		let token_address: [u8; 20] = hex!("1000000000000000000000000000000000000000");
		let assets: Assets = vec![Asset {
			id: AssetId([AccountKey20 { network: None, key: token_address }].into()),
			fun: Fungible(1000),
		}]
		.into();
		let message: Xcm<()> = vec![WithdrawAsset(assets)].into();

		let mut converter =
			XcmConverter::<MockTokenIdConvert, WETHAddress, ()>::new(&message, network);
		let result = converter.convert();
		assert_eq!(result.err(), Some(XcmConverterError::UnexpectedEndOfXcm));
	}

	#[test]
	fn xcm_converter_with_different_fee_asset_succeed() {
		let network = BridgedNetwork::get();

		let token_address: [u8; 20] = hex!("1000000000000000000000000000000000000000");
		let beneficiary_address: [u8; 20] = hex!("2000000000000000000000000000000000000000");

		let asset_location = [AccountKey20 { network: None, key: token_address }].into();
		let assets: Assets =
			vec![Asset { id: AssetId(asset_location), fun: Fungible(1000) }].into();

		let filter: AssetFilter = assets.clone().into();
		let fee_asset: Asset = Asset {
			id: AssetId([AccountKey20 { network: None, key: WETHAddress::get().0 }].into()),
			fun: Fungible(1000),
		}
		.into();

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
			PayFees { asset: fee_asset },
			WithdrawAsset(assets.clone()),
			AliasOrigin(Location::new(1, [GlobalConsensus(Polkadot), Parachain(1000)])),
			DepositAsset {
				assets: filter,
				beneficiary: AccountKey20 { network: None, key: beneficiary_address }.into(),
			},
			SetTopic([0; 32]),
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, WETHAddress, ()>::new(&message, network);
		let result = converter.convert();
		assert_eq!(result.is_ok(), true);
	}

	#[test]
	fn xcm_converter_with_fees_greater_than_reserve_succeed() {
		let network = BridgedNetwork::get();

		let token_address: [u8; 20] = hex!("1000000000000000000000000000000000000000");
		let beneficiary_address: [u8; 20] = hex!("2000000000000000000000000000000000000000");

		let asset_location: Location = [AccountKey20 { network: None, key: token_address }].into();
		let fee_asset: Asset = Asset {
			id: AssetId([AccountKey20 { network: None, key: WETHAddress::get().0 }].into()),
			fun: Fungible(1000),
		}
		.into();

		let assets: Assets =
			vec![Asset { id: AssetId(asset_location), fun: Fungible(1000) }].into();

		let filter: AssetFilter = assets.clone().into();

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
			PayFees { asset: fee_asset },
			WithdrawAsset(assets.clone()),
			AliasOrigin(Location::new(1, [GlobalConsensus(Polkadot), Parachain(1000)])),
			DepositAsset {
				assets: filter,
				beneficiary: AccountKey20 { network: None, key: beneficiary_address }.into(),
			},
			SetTopic([0; 32]),
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, WETHAddress, ()>::new(&message, network);
		let result = converter.convert();
		assert_eq!(result.is_ok(), true);
	}

	#[test]
	fn xcm_converter_convert_with_empty_xcm_yields_unexpected_end_of_xcm() {
		let network = BridgedNetwork::get();

		let message: Xcm<()> = vec![].into();

		let mut converter =
			XcmConverter::<MockTokenIdConvert, WETHAddress, ()>::new(&message, network);

		let result = converter.convert();
		assert_eq!(result.err(), Some(XcmConverterError::UnexpectedEndOfXcm));
	}

	#[test]
	fn xcm_converter_convert_with_extra_instructions_yields_end_of_xcm_message_expected() {
		let network = BridgedNetwork::get();

		let token_address: [u8; 20] = hex!("1000000000000000000000000000000000000000");
		let beneficiary_address: [u8; 20] = hex!("2000000000000000000000000000000000000000");

		let assets: Assets = vec![Asset {
			id: AssetId([AccountKey20 { network: None, key: token_address }].into()),
			fun: Fungible(1000),
		}]
		.into();
		let filter: AssetFilter = assets.clone().into();
		let fee_asset: Asset = Asset {
			id: AssetId([AccountKey20 { network: None, key: WETHAddress::get().0 }].into()),
			fun: Fungible(1000),
		}
		.into();

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
			PayFees { asset: fee_asset },
			WithdrawAsset(assets.clone()),
			AliasOrigin(Location::new(1, [GlobalConsensus(Polkadot), Parachain(1000)])),
			DepositAsset {
				assets: filter,
				beneficiary: AccountKey20 { network: None, key: beneficiary_address }.into(),
			},
			SetTopic([0; 32]),
			ClearError,
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, WETHAddress, ()>::new(&message, network);

		let result = converter.convert();
		assert_eq!(result.err(), Some(XcmConverterError::EndOfXcmMessageExpected));
	}

	#[test]
	fn xcm_converter_convert_without_withdraw_asset_yields_withdraw_expected() {
		let network = BridgedNetwork::get();

		let token_address: [u8; 20] = hex!("1000000000000000000000000000000000000000");
		let beneficiary_address: [u8; 20] = hex!("2000000000000000000000000000000000000000");

		let assets: Assets = vec![Asset {
			id: AssetId([AccountKey20 { network: None, key: token_address }].into()),
			fun: Fungible(1000),
		}]
		.into();
		let filter: AssetFilter = assets.clone().into();

		let message: Xcm<()> = vec![
			ClearOrigin,
			BuyExecution { fees: assets.get(0).unwrap().clone(), weight_limit: Unlimited },
			DepositAsset {
				assets: filter,
				beneficiary: AccountKey20 { network: None, key: beneficiary_address }.into(),
			},
			SetTopic([0; 32]),
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, WETHAddress, ()>::new(&message, network);

		let result = converter.convert();
		assert_eq!(result.err(), Some(XcmConverterError::WithdrawAssetExpected));
	}

	#[test]
	fn xcm_converter_convert_without_withdraw_asset_yields_deposit_expected() {
		let network = BridgedNetwork::get();

		let token_address: [u8; 20] = hex!("1000000000000000000000000000000000000000");

		let assets: Assets = vec![Asset {
			id: AssetId(AccountKey20 { network: None, key: token_address }.into()),
			fun: Fungible(1000),
		}]
		.into();

		let fee_asset: Asset = Asset {
			id: AssetId([AccountKey20 { network: None, key: WETHAddress::get().0 }].into()),
			fun: Fungible(1000),
		}
		.into();

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
			PayFees { asset: fee_asset },
			WithdrawAsset(assets.clone()),
			AliasOrigin(Location::new(1, [GlobalConsensus(Polkadot), Parachain(1000)])),
			SetTopic([0; 32]),
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, WETHAddress, ()>::new(&message, network);

		let result = converter.convert();
		assert_eq!(result.err(), Some(XcmConverterError::DepositAssetExpected));
	}

	#[test]
	fn xcm_converter_convert_without_assets_yields_no_reserve_assets() {
		let network = BridgedNetwork::get();

		let beneficiary_address: [u8; 20] = hex!("2000000000000000000000000000000000000000");

		let assets: Assets = vec![].into();
		let filter: AssetFilter = assets.clone().into();

		let fee_asset: Asset = Asset {
			id: AssetId([AccountKey20 { network: None, key: WETHAddress::get().0 }].into()),
			fun: Fungible(1000),
		}
		.into();

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
			PayFees { asset: fee_asset },
			AliasOrigin(Location::new(1, [GlobalConsensus(Polkadot), Parachain(1000)])),
			DepositAsset {
				assets: filter,
				beneficiary: AccountKey20 { network: None, key: beneficiary_address }.into(),
			},
			SetTopic([0; 32]),
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, WETHAddress, ()>::new(&message, network);

		let result = converter.convert();
		assert_eq!(result.err(), Some(XcmConverterError::NoReserveAssets));
	}

	#[test]
	fn xcm_converter_convert_with_two_assets_yields() {
		let network = BridgedNetwork::get();

		let token_address_1: [u8; 20] = hex!("1000000000000000000000000000000000000000");
		let token_address_2: [u8; 20] = hex!("1100000000000000000000000000000000000000");
		let beneficiary_address: [u8; 20] = hex!("2000000000000000000000000000000000000000");

		let assets: Assets = vec![
			Asset {
				id: AssetId(AccountKey20 { network: None, key: token_address_1 }.into()),
				fun: Fungible(1000),
			},
			Asset {
				id: AssetId(AccountKey20 { network: None, key: token_address_2 }.into()),
				fun: Fungible(500),
			},
		]
		.into();
		let filter: AssetFilter = assets.clone().into();
		let fee_asset: Asset = Asset {
			id: AssetId([AccountKey20 { network: None, key: WETHAddress::get().0 }].into()),
			fun: Fungible(1000),
		}
		.into();

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
			PayFees { asset: fee_asset },
			WithdrawAsset(assets.clone()),
			AliasOrigin(Location::new(1, [GlobalConsensus(Polkadot), Parachain(1000)])),
			DepositAsset {
				assets: filter,
				beneficiary: AccountKey20 { network: None, key: beneficiary_address }.into(),
			},
			SetTopic([0; 32]),
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, WETHAddress, ()>::new(&message, network);

		let result = converter.convert();
		assert_eq!(result.is_ok(), true);
	}

	#[test]
	fn xcm_converter_convert_without_consuming_filter_yields_filter_does_not_consume_all_assets() {
		let network = BridgedNetwork::get();

		let token_address: [u8; 20] = hex!("1000000000000000000000000000000000000000");
		let beneficiary_address: [u8; 20] = hex!("2000000000000000000000000000000000000000");

		let assets: Assets = vec![Asset {
			id: AssetId(AccountKey20 { network: None, key: token_address }.into()),
			fun: Fungible(1000),
		}]
		.into();
		let filter: AssetFilter = Wild(WildAsset::AllCounted(0));
		let fee_asset: Asset = Asset {
			id: AssetId([AccountKey20 { network: None, key: WETHAddress::get().0 }].into()),
			fun: Fungible(1000),
		}
		.into();

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
			PayFees { asset: fee_asset },
			WithdrawAsset(assets.clone()),
			AliasOrigin(Location::new(1, [GlobalConsensus(Polkadot), Parachain(1000)])),
			DepositAsset {
				assets: filter,
				beneficiary: AccountKey20 { network: None, key: beneficiary_address }.into(),
			},
			SetTopic([0; 32]),
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, WETHAddress, ()>::new(&message, network);

		let result = converter.convert();
		assert_eq!(result.err(), Some(XcmConverterError::FilterDoesNotConsumeAllAssets));
	}

	#[test]
	fn xcm_converter_convert_with_zero_amount_asset_yields_zero_asset_transfer() {
		let network = BridgedNetwork::get();

		let token_address: [u8; 20] = hex!("1000000000000000000000000000000000000000");
		let beneficiary_address: [u8; 20] = hex!("2000000000000000000000000000000000000000");

		let assets: Assets = vec![Asset {
			id: AssetId(AccountKey20 { network: None, key: token_address }.into()),
			fun: Fungible(0),
		}]
		.into();
		let filter: AssetFilter = Wild(WildAsset::AllCounted(1));
		let fee_asset: Asset = Asset {
			id: AssetId([AccountKey20 { network: None, key: WETHAddress::get().0 }].into()),
			fun: Fungible(1000),
		}
		.into();

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
			PayFees { asset: fee_asset },
			WithdrawAsset(assets.clone()),
			AliasOrigin(Location::new(1, [GlobalConsensus(Polkadot), Parachain(1000)])),
			DepositAsset {
				assets: filter,
				beneficiary: AccountKey20 { network: None, key: beneficiary_address }.into(),
			},
			SetTopic([0; 32]),
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, WETHAddress, ()>::new(&message, network);

		let result = converter.convert();
		assert_eq!(result.err(), Some(XcmConverterError::ZeroAssetTransfer));
	}

	#[test]
	fn xcm_converter_convert_non_ethereum_asset_yields_asset_resolution_failed() {
		let network = BridgedNetwork::get();

		let beneficiary_address: [u8; 20] = hex!("2000000000000000000000000000000000000000");

		let assets: Assets = vec![Asset {
			id: AssetId([GlobalConsensus(Polkadot), Parachain(1000), GeneralIndex(0)].into()),
			fun: Fungible(1000),
		}]
		.into();
		let filter: AssetFilter = Wild(WildAsset::AllCounted(1));
		let fee_asset: Asset = Asset {
			id: AssetId(AccountKey20 { network: None, key: WETHAddress::get().0 }.into()),
			fun: Fungible(1000),
		};

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone().into()),
			PayFees { asset: fee_asset },
			WithdrawAsset(assets.clone()),
			AliasOrigin(Location::new(1, [GlobalConsensus(Polkadot), Parachain(1000)])),
			DepositAsset {
				assets: filter,
				beneficiary: AccountKey20 { network: None, key: beneficiary_address }.into(),
			},
			SetTopic([0; 32]),
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, WETHAddress, ()>::new(&message, network);

		let result = converter.convert();
		assert_eq!(result.err(), Some(XcmConverterError::AssetResolutionFailed));
	}

	#[test]
	fn xcm_converter_convert_non_ethereum_chain_asset_yields_asset_resolution_failed() {
		let network = BridgedNetwork::get();

		let token_address: [u8; 20] = hex!("1000000000000000000000000000000000000000");
		let beneficiary_address: [u8; 20] = hex!("2000000000000000000000000000000000000000");

		let assets: Assets = vec![Asset {
			id: AssetId(
				AccountKey20 { network: Some(Ethereum { chain_id: 2 }), key: token_address }.into(),
			),
			fun: Fungible(1000),
		}]
		.into();
		let filter: AssetFilter = Wild(WildAsset::AllCounted(1));
		let fee_asset: Asset = Asset {
			id: AssetId(AccountKey20 { network: None, key: WETHAddress::get().0 }.into()),
			fun: Fungible(1000),
		};

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone().into()),
			PayFees { asset: fee_asset },
			WithdrawAsset(assets.clone()),
			AliasOrigin(Location::new(1, [GlobalConsensus(Polkadot), Parachain(1000)])),
			DepositAsset {
				assets: filter,
				beneficiary: AccountKey20 { network: None, key: beneficiary_address }.into(),
			},
			SetTopic([0; 32]),
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, WETHAddress, ()>::new(&message, network);

		let result = converter.convert();
		assert_eq!(result.err(), Some(XcmConverterError::AssetResolutionFailed));
	}

	#[test]
	fn xcm_converter_convert_non_ethereum_chain_yields_asset_resolution_failed() {
		let network = BridgedNetwork::get();

		let token_address: [u8; 20] = hex!("1000000000000000000000000000000000000000");
		let beneficiary_address: [u8; 20] = hex!("2000000000000000000000000000000000000000");

		let assets: Assets = vec![Asset {
			id: AssetId(
				[AccountKey20 { network: Some(NonBridgedNetwork::get()), key: token_address }]
					.into(),
			),
			fun: Fungible(1000),
		}]
		.into();
		let filter: AssetFilter = Wild(WildAsset::AllCounted(1));
		let fee_asset: Asset = Asset {
			id: AssetId(AccountKey20 { network: None, key: WETHAddress::get().0 }.into()),
			fun: Fungible(1000),
		};

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone().into()),
			PayFees { asset: fee_asset },
			WithdrawAsset(assets.clone()),
			AliasOrigin(Location::new(1, [GlobalConsensus(Polkadot), Parachain(1000)])),
			DepositAsset {
				assets: filter,
				beneficiary: AccountKey20 { network: None, key: beneficiary_address }.into(),
			},
			SetTopic([0; 32]),
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, WETHAddress, ()>::new(&message, network);

		let result = converter.convert();
		assert_eq!(result.err(), Some(XcmConverterError::AssetResolutionFailed));
	}

	#[test]
	fn xcm_converter_convert_with_non_ethereum_beneficiary_yields_beneficiary_resolution_failed() {
		let network = BridgedNetwork::get();

		let token_address: [u8; 20] = hex!("1000000000000000000000000000000000000000");

		let beneficiary_address: [u8; 32] =
			hex!("2000000000000000000000000000000000000000000000000000000000000000");

		let assets: Assets = vec![Asset {
			id: AssetId(AccountKey20 { network: None, key: token_address }.into()),
			fun: Fungible(1000),
		}]
		.into();
		let filter: AssetFilter = Wild(WildAsset::AllCounted(1));
		let fee_asset: Asset = Asset {
			id: AssetId(AccountKey20 { network: None, key: WETHAddress::get().0 }.into()),
			fun: Fungible(1000),
		};

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone().into()),
			PayFees { asset: fee_asset },
			WithdrawAsset(assets.clone()),
			AliasOrigin(Location::new(1, [GlobalConsensus(Polkadot), Parachain(1000)])),
			DepositAsset {
				assets: filter,
				beneficiary: AccountId32 { network: Some(Polkadot), id: beneficiary_address }
					.into(),
			},
			SetTopic([0; 32]),
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, WETHAddress, ()>::new(&message, network);

		let result = converter.convert();
		assert_eq!(result.err(), Some(XcmConverterError::BeneficiaryResolutionFailed));
	}

	#[test]
	fn xcm_converter_convert_with_non_ethereum_chain_beneficiary_yields_beneficiary_resolution_failed(
	) {
		let network = BridgedNetwork::get();

		let token_address: [u8; 20] = hex!("1000000000000000000000000000000000000000");
		let beneficiary_address: [u8; 20] = hex!("2000000000000000000000000000000000000000");

		let assets: Assets = vec![Asset {
			id: AssetId(AccountKey20 { network: None, key: token_address }.into()),
			fun: Fungible(1000),
		}]
		.into();
		let filter: AssetFilter = Wild(WildAsset::AllCounted(1));
		let fee_asset: Asset = Asset {
			id: AssetId(AccountKey20 { network: None, key: WETHAddress::get().0 }.into()),
			fun: Fungible(1000),
		};

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
			PayFees { asset: fee_asset },
			WithdrawAsset(assets.clone()),
			AliasOrigin(Location::new(1, [GlobalConsensus(Polkadot), Parachain(1000)])),
			DepositAsset {
				assets: filter,
				beneficiary: AccountKey20 {
					network: Some(Ethereum { chain_id: 2 }),
					key: beneficiary_address,
				}
				.into(),
			},
			SetTopic([0; 32]),
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, WETHAddress, ()>::new(&message, network);

		let result = converter.convert();
		assert_eq!(result.err(), Some(XcmConverterError::BeneficiaryResolutionFailed));
	}

	#[test]
	fn test_describe_asset_hub() {
		let legacy_location: Location = Location::new(0, [Parachain(1000)]);
		let legacy_agent_id = AgentIdOf::convert_location(&legacy_location).unwrap();
		assert_eq!(
			legacy_agent_id,
			hex!("72456f48efed08af20e5b317abf8648ac66e86bb90a411d9b0b713f7364b75b4").into()
		);
		let location: Location = Location::new(1, [Parachain(1000)]);
		let agent_id = AgentIdOf::convert_location(&location).unwrap();
		assert_eq!(
			agent_id,
			hex!("81c5ab2571199e3188135178f3c2c8e2d268be1313d029b30f534fa579b69b79").into()
		)
	}

	#[test]
	fn test_describe_here() {
		let location: Location = Location::new(0, []);
		let agent_id = AgentIdOf::convert_location(&location).unwrap();
		assert_eq!(
			agent_id,
			hex!("03170a2e7597b7b7e3d84c05391d139a62b157e78786d8c082f29dcf4c111314").into()
		)
	}

	#[test]
	fn xcm_converter_transfer_native_token_success() {
		let network = BridgedNetwork::get();

		let beneficiary_address: [u8; 20] = hex!("2000000000000000000000000000000000000000");

		let amount = 1000000;
		let asset_location = Location::new(1, [GlobalConsensus(ByGenesis(WESTEND_GENESIS_HASH))]);
		let token_id = TokenIdOf::convert_location(&asset_location).unwrap();

		let assets: Assets =
			vec![Asset { id: AssetId(asset_location.clone()), fun: Fungible(amount) }].into();
		let filter: AssetFilter = assets.clone().into();
		let fee_asset: Asset = Asset {
			id: AssetId(AccountKey20 { network: None, key: WETHAddress::get().0 }.into()),
			fun: Fungible(1000),
		};

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
			PayFees { asset: fee_asset },
			ReserveAssetDeposited(assets.clone()),
			AliasOrigin(Location::new(1, [GlobalConsensus(Polkadot), Parachain(1000)])),
			DepositAsset {
				assets: filter,
				beneficiary: AccountKey20 { network: None, key: beneficiary_address }.into(),
			},
			SetTopic([0; 32]),
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, WETHAddress, ()>::new(&message, network);
		let expected_payload =
			Command::MintForeignToken { recipient: beneficiary_address.into(), amount, token_id };
		let expected_message = Message {
			origin_location: Location::new(1, [GlobalConsensus(Polkadot), Parachain(1000)]),
			id: [0; 32].into(),
			origin: hex!("aa16eddac8725928eaeda4aae518bf10d02bee80382517d21464a5cdf8d1d8e1").into(),
			fee: 1000,
			commands: BoundedVec::try_from(vec![expected_payload]).unwrap(),
		};
		let result = converter.convert();
		assert_eq!(result, Ok(expected_message));
	}

	#[test]
	fn xcm_converter_transfer_native_token_with_invalid_location_will_fail() {
		let network = BridgedNetwork::get();

		let beneficiary_address: [u8; 20] = hex!("2000000000000000000000000000000000000000");

		let amount = 1000000;
		// Invalid asset location from a different consensus
		let asset_location = Location {
			parents: 2,
			interior: [GlobalConsensus(ByGenesis(WESTEND_GENESIS_HASH))].into(),
		};

		let assets: Assets =
			vec![Asset { id: AssetId(asset_location), fun: Fungible(amount) }].into();
		let filter: AssetFilter = assets.clone().into();

		let fee_asset: Asset = Asset {
			id: AssetId(AccountKey20 { network: None, key: WETHAddress::get().0 }.into()),
			fun: Fungible(1000),
		};

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
			PayFees { asset: fee_asset },
			ReserveAssetDeposited(assets.clone()),
			AliasOrigin(Location::new(1, [GlobalConsensus(Polkadot), Parachain(1000)])),
			DepositAsset {
				assets: filter,
				beneficiary: AccountKey20 { network: None, key: beneficiary_address }.into(),
			},
			SetTopic([0; 32]),
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, WETHAddress, ()>::new(&message, network);
		let result = converter.convert();
		assert_eq!(result.err(), Some(XcmConverterError::InvalidAsset));
	}
}
