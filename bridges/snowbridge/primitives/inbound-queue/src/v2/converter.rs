// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Converts messages from Solidity ABI-encoding to XCM

use codec::{Decode, DecodeLimit, Encode};
use core::marker::PhantomData;
use snowbridge_core::TokenId;
use sp_core::{Get, RuntimeDebug, H160};
use sp_runtime::traits::MaybeEquivalence;
use sp_std::prelude::*;
use xcm::{
	prelude::{Junction::*, *}, MAX_XCM_DECODE_DEPTH
};
use crate::v2::LOG_TARGET;
use sp_io::hashing::blake2_256;

use super::message::*;


/// Reason why a message conversion failed.
#[derive(Copy, Clone, Encode, Decode, RuntimeDebug, PartialEq)]
pub enum ConvertMessageError {
	/// Invalid foreign ERC-20 token ID
	InvalidAsset,
	/// Cannot reachor a foreign ERC-20 asset location.
	CannotReanchor,
}

pub trait ConvertMessage {
	fn convert(
		message: Message,
	) -> Result<Xcm<()>, ConvertMessageError>;
}

pub struct MessageToXcm<
	EthereumNetwork,
	InboundQueueLocation,
	ConvertAssetId,
	GatewayProxyAddress,
	EthereumUniversalLocation,
	GlobalAssetHubLocation,
> where
	EthereumNetwork: Get<NetworkId>,
	InboundQueueLocation: Get<InteriorLocation>,
	ConvertAssetId: MaybeEquivalence<TokenId, Location>,
	GatewayProxyAddress: Get<H160>,
	EthereumUniversalLocation: Get<InteriorLocation>,
	GlobalAssetHubLocation: Get<Location>,
{
	_phantom: PhantomData<(
		EthereumNetwork,
		InboundQueueLocation,
		ConvertAssetId,
		GatewayProxyAddress,
		EthereumUniversalLocation,
		GlobalAssetHubLocation,
	)>,
}

impl<
		EthereumNetwork,
		InboundQueueLocation,
		ConvertAssetId,
		GatewayProxyAddress,
		EthereumUniversalLocation,
		GlobalAssetHubLocation,
	> ConvertMessage
	for MessageToXcm<
		EthereumNetwork,
		InboundQueueLocation,
		ConvertAssetId,
		GatewayProxyAddress,
		EthereumUniversalLocation,
		GlobalAssetHubLocation,
	>
where
	EthereumNetwork: Get<NetworkId>,
	InboundQueueLocation: Get<InteriorLocation>,
	ConvertAssetId: MaybeEquivalence<TokenId, Location>,
	GatewayProxyAddress: Get<H160>,
	EthereumUniversalLocation: Get<InteriorLocation>,
	GlobalAssetHubLocation: Get<Location>,
{
	fn convert(
		message: Message
	) -> Result<Xcm<()>, ConvertMessageError> {
		let mut message_xcm: Xcm<()> = Xcm::new();
		if message.xcm.len() > 0 {
			// Allow xcm decode failure so that assets can be trapped on AH instead of this
			// message failing but funds are already locked on Ethereum.
			if let Ok(versioned_xcm) = VersionedXcm::<()>::decode_with_depth_limit(
				MAX_XCM_DECODE_DEPTH,
				&mut message.xcm.as_ref(),
			) {
				if let Ok(decoded_xcm) = versioned_xcm.try_into() {
					message_xcm = decoded_xcm;
				} else {
					log::error!(target: LOG_TARGET,"unable to decode xcm");
				}
			} else {
				log::error!(target: LOG_TARGET,"unable to decode versioned xcm");
			}
		}

		log::trace!(target: LOG_TARGET,"xcm decoded as {:?}", message_xcm);

		let network = EthereumNetwork::get();

		// use eth as asset
		let fee_asset_id = Location::new(2, [GlobalConsensus(EthereumNetwork::get())]);
		let fee: Asset = (fee_asset_id.clone(), message.execution_fee).into();
		let eth: Asset =
			(fee_asset_id.clone(), message.execution_fee.saturating_add(message.value)).into();
		let mut instructions = vec![
			DescendOrigin(InboundQueueLocation::get()),
			UniversalOrigin(GlobalConsensus(network)),
			ReserveAssetDeposited(eth.into()),
			PayFees { asset: fee },
		];
		let mut reserve_assets = vec![];
		let mut withdraw_assets = vec![];

		// Let origin account transact on AH directly to reclaim assets and surplus fees.
		// This will be possible when AH gets full EVM support
		let default_claimer = Location::new(0, [
			AccountKey20 {
				// Set network to `None` to support future Plaza EVM chainid by default.
				network: None,
				// Ethereum account ID
				key: message.origin.as_fixed_bytes().clone()
			}
		]);

		// Derive an asset claimer, either from the origin location, or if specified in the message
		// in the message
		let claimer = message.claimer.map_or(
			default_claimer.clone(),
			|claimer_bytes| Location::decode(&mut claimer_bytes.as_ref()).unwrap_or(default_claimer.clone())
		);

		instructions.push(
			SetHints {
				hints: vec![
					AssetClaimer { location: claimer.clone() }
				].try_into().expect("checked statically, qed")
			}
		);

		for asset in &message.assets {
			match asset {
				EthereumAsset::NativeTokenERC20 { token_id, value } => {
					let token_location: Location = Location::new(
						2,
						[
							GlobalConsensus(EthereumNetwork::get()),
							AccountKey20 { network: None, key: (*token_id).into() },
						],
					);
					let asset: Asset = (token_location, *value).into();
					reserve_assets.push(asset);
				},
				EthereumAsset::ForeignTokenERC20 { token_id, value } => {
					let asset_loc = ConvertAssetId::convert(&token_id)
						.ok_or(ConvertMessageError::InvalidAsset)?;
					let mut reanchored_asset_loc = asset_loc.clone();
					reanchored_asset_loc
						.reanchor(&GlobalAssetHubLocation::get(), &EthereumUniversalLocation::get())
						.map_err(|_| ConvertMessageError::CannotReanchor)?;
					let asset: Asset = (reanchored_asset_loc, *value).into();
					withdraw_assets.push(asset);
				},
			}
		}

		if reserve_assets.len() > 0 {
			instructions.push(ReserveAssetDeposited(reserve_assets.into()));
		}
		if withdraw_assets.len() > 0 {
			instructions.push(WithdrawAsset(withdraw_assets.into()));
		}

		// If the message origin is not the gateway proxy contract, set the origin to
		// the original sender on Ethereum. Important to be before the arbitrary XCM that is
		// appended to the message on the next line.
		if message.origin != GatewayProxyAddress::get() {
			instructions.push(DescendOrigin(
				AccountKey20 { key: message.origin.into(), network: None }.into(),
			));
		}

		let topic = blake2_256(&("snowbridge-inbound-queue:v2", message.nonce).encode());

		// Add the XCM sent in the message to the end of the xcm instruction
		instructions.extend(message_xcm.0);

		instructions.push(SetTopic(topic.into()));
		instructions.push(RefundSurplus);
		// Refund excess fees to the claimer, if present, otherwise to the relayer.
		instructions.push(DepositAsset {
			assets: Wild(AllOf { id: AssetId(fee_asset_id.into()), fun: WildFungible }),
			beneficiary: claimer,
		});

		log::trace!(target: LOG_TARGET,"converted message to xcm {:?}", instructions);
		Ok(instructions.into())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	use codec::Encode;
	use frame_support::{assert_err, assert_ok, parameter_types};
	use hex_literal::hex;
	use snowbridge_core::TokenId;
	use sp_core::{H160, H256};
	use sp_runtime::traits::MaybeEquivalence;
	use xcm::opaque::latest::WESTEND_GENESIS_HASH;
	const GATEWAY_ADDRESS: [u8; 20] = hex!["eda338e4dc46038493b885327842fd3e301cab39"];
	parameter_types! {
		pub const EthereumNetwork: xcm::v5::NetworkId = xcm::v5::NetworkId::Ethereum { chain_id: 11155111 };
		pub const GatewayAddress: H160 = H160(GATEWAY_ADDRESS);
		pub InboundQueueLocation: InteriorLocation = [PalletInstance(84)].into();
		pub UniversalLocation: InteriorLocation =
			[GlobalConsensus(ByGenesis(WESTEND_GENESIS_HASH)), Parachain(1002)].into();
		pub AssetHubFromEthereum: Location = Location::new(1,[GlobalConsensus(ByGenesis(WESTEND_GENESIS_HASH)),Parachain(1000)]);
	}

	pub struct MockTokenIdConvert;
	impl MaybeEquivalence<TokenId, Location> for MockTokenIdConvert {
		fn convert(_id: &TokenId) -> Option<Location> {
			Some(Location::parent())
		}
		fn convert_back(_loc: &Location) -> Option<TokenId> {
			None
		}
	}

	pub struct MockFailedTokenConvert;
	impl MaybeEquivalence<TokenId, Location> for MockFailedTokenConvert {
		fn convert(_id: &TokenId) -> Option<Location> {
			None
		}
		fn convert_back(_loc: &Location) -> Option<TokenId> {
			None
		}
	}

	#[test]
	fn test_successful_message() {
		let origin: H160 = hex!("29e3b139f4393adda86303fcdaa35f60bb7092bf").into();
		let native_token_id: H160 = hex!("5615deb798bb3e4dfa0139dfa1b3d433cc23b72f").into();
		let foreign_token_id: H256 =
			hex!("37a6c666da38711a963d938eafdd09314fd3f95a96a3baffb55f26560f4ecdd8").into();
		let beneficiary: Location =
			hex!("908783d8cd24c9e02cee1d26ab9c46d458621ad0150b626c536a40b9df3f09c6").into();
		let token_value = 3_000_000_000_000u128;
		let assets = vec![
			EthereumAsset::NativeTokenERC20 { token_id: native_token_id, value: token_value },
			EthereumAsset::ForeignTokenERC20 { token_id: foreign_token_id, value: token_value },
		];
		let instructions = vec![
			RefundSurplus,
			DepositAsset { assets: Wild(AllCounted(1).into()), beneficiary: beneficiary.clone() },
		];
		let xcm: Xcm<()> = instructions.into();
		let versioned_xcm = VersionedXcm::V5(xcm);
		let claimer_location = Location::new(0, AccountId32 { network: None, id: H256::random().into() });
		let claimer: Option<Vec<u8>> = Some(claimer_location.clone().encode());
		let value = 6_000_000_000_000u128;
		let execution_fee = 1_000_000_000_000u128;
		let relayer_fee = 5_000_000_000_000u128;

		let message = Message {
			gateway: H160::zero(),
			nonce: 0,
			origin: origin.clone(),
			assets,
			xcm: versioned_xcm.encode(),
			claimer,
			value,
			execution_fee,
			relayer_fee,
		};

		let result = MessageToXcm::<
			EthereumNetwork,
			InboundQueueLocation,
			MockTokenIdConvert,
			GatewayAddress,
			UniversalLocation,
			AssetHubFromEthereum,
		>::convert(message, [0; 32]);

		assert_ok!(result.clone());

		let xcm = result.unwrap();

		let mut instructions = xcm.into_iter();

		let mut asset_claimer_found = false;
		let mut pay_fees_found = false;
		let mut descend_origin_found = 0;
		let mut reserve_deposited_found = 0;
		let mut withdraw_assets_found = 0;
		let mut refund_surplus_found = 0;
		let mut deposit_asset_found = 0;
		while let Some(instruction) = instructions.next() {
			if let SetHints { ref hints } = instruction {
				if let Some(AssetClaimer { ref location }) = hints.clone().into_iter().next() {
					assert_eq!(claimer_location, location.clone());
					asset_claimer_found = true;
				}
			}
			if let DescendOrigin(ref location) = instruction {
				descend_origin_found = descend_origin_found + 1;
				// The second DescendOrigin should be the message.origin (sender)
				if descend_origin_found == 2 {
					let junctions: Junctions =
						AccountKey20 { key: origin.into(), network: None }.into();
					assert_eq!(junctions, location.clone());
				}
			}
			if let PayFees { ref asset } = instruction {
				let fee_asset = Location::new(2, [GlobalConsensus(EthereumNetwork::get())]);
				assert_eq!(asset.id, AssetId(fee_asset));
				assert_eq!(asset.fun, Fungible(execution_fee));
				pay_fees_found = true;
			}
			if let ReserveAssetDeposited(ref reserve_assets) = instruction {
				reserve_deposited_found = reserve_deposited_found + 1;
				if reserve_deposited_found == 1 {
					let fee_asset = Location::new(2, [GlobalConsensus(EthereumNetwork::get())]);
					let fee: Asset = (fee_asset, execution_fee + value).into();
					let fee_assets: Assets = fee.into();
					assert_eq!(fee_assets, reserve_assets.clone());
				}
				if reserve_deposited_found == 2 {
					let token_asset = Location::new(
						2,
						[
							GlobalConsensus(EthereumNetwork::get()),
							AccountKey20 { network: None, key: native_token_id.into() },
						],
					);
					let token: Asset = (token_asset, token_value).into();
					let token_assets: Assets = token.into();
					assert_eq!(token_assets, reserve_assets.clone());
				}
			}
			if let WithdrawAsset(ref withdraw_assets) = instruction {
				withdraw_assets_found = withdraw_assets_found + 1;
				let token_asset = Location::new(2, Here);
				let token: Asset = (token_asset, token_value).into();
				let token_assets: Assets = token.into();
				assert_eq!(token_assets, withdraw_assets.clone());
			}
			if let RefundSurplus = instruction {
				refund_surplus_found = refund_surplus_found + 1;
			}
			if let DepositAsset { ref assets, beneficiary: deposit_beneficiary } = instruction {
				deposit_asset_found = deposit_asset_found + 1;
				if deposit_asset_found == 1 {
					assert_eq!(AssetFilter::from( Wild(AllCounted(1).into())), assets.clone());
					assert_eq!(deposit_beneficiary, beneficiary);
				} else if deposit_asset_found == 2 {
					let fee_asset_id = Location::new(2, [GlobalConsensus(EthereumNetwork::get())]);
					assert_eq!(Wild(AllOf { id: AssetId(fee_asset_id.into()), fun: WildFungible }), assets.clone());
					assert_eq!(deposit_beneficiary, claimer_location);
				}

			}
		}

		// SetAssetClaimer must be in the message.
		assert!(asset_claimer_found);
		// PayFees must be in the message.
		assert!(pay_fees_found);
		// The first DescendOrigin to descend into the InboundV2 pallet index and the DescendOrigin
		// into the message.origin
		assert!(descend_origin_found == 2);
		// Expecting two ReserveAssetDeposited instructions, one for the fee and one for the token
		// being transferred.
		assert!(reserve_deposited_found == 2);
		// Expecting one WithdrawAsset for the foreign ERC-20
		assert!(withdraw_assets_found == 1);
		// One added by the user, one appended to the message in the converter.
		assert!(refund_surplus_found == 2);
		// Deposit asset added by the converter and user
		assert!(deposit_asset_found == 2);
	}

	#[test]
	fn test_message_with_gateway_origin_does_not_descend_origin_into_sender() {
		let origin_account =
			Location::new(0, [AccountId32 { network: None, id: H256::random().into() }]);
		let origin: H160 = GatewayAddress::get();
		let native_token_id: H160 = hex!("5615deb798bb3e4dfa0139dfa1b3d433cc23b72f").into();
		let foreign_token_id: H256 =
			hex!("37a6c666da38711a963d938eafdd09314fd3f95a96a3baffb55f26560f4ecdd8").into();
		let beneficiary =
			hex!("908783d8cd24c9e02cee1d26ab9c46d458621ad0150b626c536a40b9df3f09c6").into();
		let message_id: H256 =
			hex!("8b69c7e376e28114618e829a7ec768dbda28357d359ba417a3bd79b11215059d").into();
		let token_value = 3_000_000_000_000u128;
		let assets = vec![
			EthereumAsset::NativeTokenERC20 { token_id: native_token_id, value: token_value },
			EthereumAsset::ForeignTokenERC20 { token_id: foreign_token_id, value: token_value },
		];
		let instructions = vec![
			DepositAsset { assets: Wild(AllCounted(1).into()), beneficiary },
			SetTopic(message_id.into()),
		];
		let xcm: Xcm<()> = instructions.into();
		let versioned_xcm = VersionedXcm::V5(xcm);
		let claimer_account = AccountId32 { network: None, id: H256::random().into() };
		let claimer: Option<Vec<u8>> = Some(claimer_account.clone().encode());
		let value = 6_000_000_000_000u128;
		let execution_fee = 1_000_000_000_000u128;
		let relayer_fee = 5_000_000_000_000u128;

		let message = Message {
			gateway: H160::zero(),
			nonce: 0,
			origin: origin.clone(),
			assets,
			xcm: versioned_xcm.encode(),
			claimer,
			value,
			execution_fee,
			relayer_fee,
		};

		let result = MessageToXcm::<
			EthereumNetwork,
			InboundQueueLocation,
			MockTokenIdConvert,
			GatewayAddress,
			UniversalLocation,
			AssetHubFromEthereum,
		>::convert(message, [0; 32]);

		assert_ok!(result.clone());

		let xcm = result.unwrap();

		let mut instructions = xcm.into_iter();
		let mut commands_found = 0;
		while let Some(instruction) = instructions.next() {
			if let DescendOrigin(ref _location) = instruction {
				commands_found = commands_found + 1;
			}
		}
		// There should only be 1 DescendOrigin in the message.
		assert!(commands_found == 1);
	}

	#[test]
	fn test_invalid_foreign_erc20() {
		let origin: H160 = hex!("29e3b139f4393adda86303fcdaa35f60bb7092bf").into();
		let token_id: H256 =
			hex!("37a6c666da38711a963d938eafdd09314fd3f95a96a3baffb55f26560f4ecdd8").into();
		let beneficiary =
			hex!("908783d8cd24c9e02cee1d26ab9c46d458621ad0150b626c536a40b9df3f09c6").into();
		let message_id: H256 =
			hex!("8b69c7e376e28114618e829a7ec768dbda28357d359ba417a3bd79b11215059d").into();
		let token_value = 3_000_000_000_000u128;
		let assets = vec![EthereumAsset::ForeignTokenERC20 { token_id, value: token_value }];
		let instructions = vec![
			DepositAsset { assets: Wild(AllCounted(1).into()), beneficiary },
			SetTopic(message_id.into()),
		];
		let xcm: Xcm<()> = instructions.into();
		let versioned_xcm = VersionedXcm::V5(xcm);
		let claimer_account = AccountId32 { network: None, id: H256::random().into() };
		let claimer: Option<Vec<u8>> = Some(claimer_account.clone().encode());
		let value = 6_000_000_000_000u128;
		let execution_fee = 1_000_000_000_000u128;
		let relayer_fee = 5_000_000_000_000u128;

		let message = Message {
			gateway: H160::zero(),
			nonce: 0,
			origin,
			assets,
			xcm: versioned_xcm.encode(),
			claimer,
			value,
			execution_fee,
			relayer_fee,
		};

		let result = MessageToXcm::<
			EthereumNetwork,
			InboundQueueLocation,
			MockFailedTokenConvert,
			GatewayAddress,
			UniversalLocation,
			AssetHubFromEthereum,
		>::convert(message, [0; 32]);

		assert_err!(result.clone(), ConvertMessageError::InvalidAsset);
	}

	#[test]
	fn test_invalid_claimer() {
		let origin_account =
			Location::new(0, [AccountId32 { network: None, id: H256::random().into() }]);
		let origin: H160 = hex!("29e3b139f4393adda86303fcdaa35f60bb7092bf").into();
		let token_id: H256 =
			hex!("37a6c666da38711a963d938eafdd09314fd3f95a96a3baffb55f26560f4ecdd8").into();
		let beneficiary =
			hex!("908783d8cd24c9e02cee1d26ab9c46d458621ad0150b626c536a40b9df3f09c6").into();
		let message_id: H256 =
			hex!("8b69c7e376e28114618e829a7ec768dbda28357d359ba417a3bd79b11215059d").into();
		let token_value = 3_000_000_000_000u128;
		let assets = vec![EthereumAsset::ForeignTokenERC20 { token_id, value: token_value }];
		let instructions = vec![
			DepositAsset { assets: Wild(AllCounted(1).into()), beneficiary },
			SetTopic(message_id.into()),
		];
		let xcm: Xcm<()> = instructions.into();
		let versioned_xcm = VersionedXcm::V5(xcm);
		// Invalid claimer location, cannot be decoded into a Junction
		let claimer: Option<Vec<u8>> =
			Some(hex!("43581a7d43757158624921ab0e9e112a1d7da93cbe64782d563e8e1144a06c3c").to_vec());
		let value = 6_000_000_000_000u128;
		let execution_fee = 1_000_000_000_000u128;
		let relayer_fee = 5_000_000_000_000u128;

		let message = Message {
			gateway: H160::zero(),
			nonce: 0,
			origin,
			assets,
			xcm: versioned_xcm.encode(),
			claimer,
			value,
			execution_fee,
			relayer_fee,
		};

		let result = MessageToXcm::<
			EthereumNetwork,
			InboundQueueLocation,
			MockTokenIdConvert,
			GatewayAddress,
			UniversalLocation,
			AssetHubFromEthereum,
		>::convert(message, [0; 32]);

		// Invalid claimer does not break the message conversion
		assert_ok!(result.clone());

		let xcm = result.unwrap();

		let mut result_instructions = xcm.clone().into_iter();

		let mut found = false;
		while let Some(instruction) = result_instructions.next() {
			if let SetHints { ref hints } = instruction {
				if let Some(AssetClaimer { .. }) = hints.clone().into_iter().next() {
					found = true;
					break;
				}
			}
		}
		// SetAssetClaimer should not be in the message.
		assert!(!found);

		// Find the last two instructions to check the appendix is correct.
		let mut second_last = None;
		let mut last = None;

		for instruction in xcm.into_iter() {
			second_last = last;
			last = Some(instruction);
		}

		// Check if both instructions are found
		assert!(last.is_some());
		assert!(second_last.is_some());

		let fee_asset = Location::new(2, [GlobalConsensus(EthereumNetwork::get())]);
		assert_eq!(
			last,
			Some(DepositAsset {
				assets: Wild(AllOf { id: AssetId(fee_asset), fun: WildFungibility::Fungible }),
				// beneficiary is the relayer
				beneficiary: origin_account
			})
		);
	}

	#[test]
	fn test_invalid_xcm() {
		let origin: H160 = hex!("29e3b139f4393adda86303fcdaa35f60bb7092bf").into();
		let token_id: H256 =
			hex!("37a6c666da38711a963d938eafdd09314fd3f95a96a3baffb55f26560f4ecdd8").into();
		let token_value = 3_000_000_000_000u128;
		let assets = vec![EthereumAsset::ForeignTokenERC20 { token_id, value: token_value }];
		// invalid xcm
		let versioned_xcm = hex!("8b69c7e376e28114618e829a7ec7").to_vec();
		let claimer_account = AccountId32 { network: None, id: H256::random().into() };
		let claimer: Option<Vec<u8>> = Some(claimer_account.clone().encode());
		let value = 6_000_000_000_000u128;
		let execution_fee = 1_000_000_000_000u128;
		let relayer_fee = 5_000_000_000_000u128;

		let message = Message {
			gateway: H160::zero(),
			nonce: 0,
			origin,
			assets,
			xcm: versioned_xcm,
			claimer: Some(claimer.encode()),
			value,
			execution_fee,
			relayer_fee,
		};

		let result = MessageToXcm::<
			EthereumNetwork,
			InboundQueueLocation,
			MockTokenIdConvert,
			GatewayAddress,
			UniversalLocation,
			AssetHubFromEthereum,
		>::convert(message, [0; 32]);

		// Invalid xcm does not break the message, allowing funds to be trapped on AH.
		assert_ok!(result.clone());
	}
}
