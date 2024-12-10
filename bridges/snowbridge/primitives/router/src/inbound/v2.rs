// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Converts messages from Ethereum to XCM messages

use codec::{Decode, DecodeLimit, Encode};
use core::marker::PhantomData;
use frame_support::PalletError;
use scale_info::TypeInfo;
use snowbridge_core::TokenId;
use sp_core::{Get, RuntimeDebug, H160, H256};
use sp_runtime::traits::MaybeEquivalence;
use sp_std::prelude::*;
use xcm::{
	prelude::{Asset as XcmAsset, Junction::AccountKey20, *},
	MAX_XCM_DECODE_DEPTH,
};

const LOG_TARGET: &str = "snowbridge-router-primitives";

/// The ethereum side sends messages which are transcoded into XCM on BH. These messages are
/// self-contained, in that they can be transcoded using only information in the message.
#[derive(Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct Message {
	/// The origin address
	pub origin: H160,
	/// The assets
	pub assets: Vec<Asset>,
	/// The command originating from the Gateway contract
	pub xcm: Vec<u8>,
	/// The claimer in the case that funds get trapped.
	pub claimer: Option<Vec<u8>>,
	/// The full value of the assets.
	pub value: u128,
	/// Fee in eth to cover the xcm execution on AH.
	pub execution_fee: u128,
	/// Relayer reward in eth. Needs to cover all costs of sending a message.
	pub relayer_fee: u128,
}

/// An asset that will be transacted on AH. The asset will be reserved/withdrawn and placed into
/// the holding register. The user needs to provide additional xcm to deposit the asset
/// in a beneficiary account.
#[derive(Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum Asset {
	NativeTokenERC20 {
		/// The native token ID
		token_id: H160,
		/// The monetary value of the asset
		value: u128,
	},
	ForeignTokenERC20 {
		/// The foreign token ID
		token_id: H256,
		/// The monetary value of the asset
		value: u128,
	},
}

/// Reason why a message conversion failed.
#[derive(Copy, Clone, TypeInfo, PalletError, Encode, Decode, RuntimeDebug, PartialEq)]
pub enum ConvertMessageError {
	/// Invalid foreign ERC-20 token ID
	InvalidAsset,
	/// Cannot reachor a foreign ERC-20 asset location.
	CannotReanchor,
}

pub trait ConvertMessage {
	fn convert(
		message: Message,
		origin_account: Location,
	) -> Result<(Xcm<()>, u128), ConvertMessageError>;
}

pub struct MessageToXcm<
	EthereumNetwork,
	InboundQueuePalletInstance,
	ConvertAssetId,
	WethAddress,
	GatewayProxyAddress,
	EthereumUniversalLocation,
	GlobalAssetHubLocation,
> where
	EthereumNetwork: Get<NetworkId>,
	InboundQueuePalletInstance: Get<u8>,
	ConvertAssetId: MaybeEquivalence<TokenId, Location>,
	WethAddress: Get<H160>,
	GatewayProxyAddress: Get<H160>,
	EthereumUniversalLocation: Get<InteriorLocation>,
	GlobalAssetHubLocation: Get<Location>,
{
	_phantom: PhantomData<(
		EthereumNetwork,
		InboundQueuePalletInstance,
		ConvertAssetId,
		WethAddress,
		GatewayProxyAddress,
		EthereumUniversalLocation,
		GlobalAssetHubLocation,
	)>,
}

impl<
		EthereumNetwork,
		InboundQueuePalletInstance,
		ConvertAssetId,
		WethAddress,
		GatewayProxyAddress,
		EthereumUniversalLocation,
		GlobalAssetHubLocation,
	> ConvertMessage
	for MessageToXcm<
		EthereumNetwork,
		InboundQueuePalletInstance,
		ConvertAssetId,
		WethAddress,
		GatewayProxyAddress,
		EthereumUniversalLocation,
		GlobalAssetHubLocation,
	>
where
	EthereumNetwork: Get<NetworkId>,
	InboundQueuePalletInstance: Get<u8>,
	ConvertAssetId: MaybeEquivalence<TokenId, Location>,
	WethAddress: Get<H160>,
	GatewayProxyAddress: Get<H160>,
	EthereumUniversalLocation: Get<InteriorLocation>,
	GlobalAssetHubLocation: Get<Location>,
{
	fn convert(
		message: Message,
		origin_account_location: Location,
	) -> Result<(Xcm<()>, u128), ConvertMessageError> {
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
				}
			}
		}

		log::debug!(target: LOG_TARGET,"xcm decoded as {:?}", message_xcm);

		let network = EthereumNetwork::get();

		// use weth as asset
		let fee_asset = Location::new(
			2,
			[
				GlobalConsensus(EthereumNetwork::get()),
				AccountKey20 { network: None, key: WethAddress::get().into() },
			],
		);
		let fee: XcmAsset = (fee_asset.clone(), message.execution_fee).into();
		let mut instructions = vec![
			DescendOrigin(PalletInstance(InboundQueuePalletInstance::get()).into()),
			UniversalOrigin(GlobalConsensus(network)),
			ReserveAssetDeposited(fee.clone().into()),
			PayFees { asset: fee },
		];
		let mut reserve_assets = vec![];
		let mut withdraw_assets = vec![];

		let mut refund_surplus_to = origin_account_location;

		if let Some(claimer) = message.claimer {
			// If the claimer can be decoded, add it to the message. If the claimer decoding fails,
			// do not add it to the message, because it will cause the xcm to fail.
			if let Ok(claimer) = Junction::decode(&mut claimer.as_ref()) {
				let claimer_location: Location = Location::new(0, [claimer.into()]);
				refund_surplus_to = claimer_location.clone();
				instructions.push(SetAssetClaimer { location: claimer_location });
			}
		}

		for asset in &message.assets {
			match asset {
				Asset::NativeTokenERC20 { token_id, value } => {
					let token_location: Location = Location::new(
						2,
						[
							GlobalConsensus(EthereumNetwork::get()),
							AccountKey20 { network: None, key: (*token_id).into() },
						],
					);
					let asset: XcmAsset = (token_location, *value).into();
					reserve_assets.push(asset);
				},
				Asset::ForeignTokenERC20 { token_id, value } => {
					let asset_loc = ConvertAssetId::convert(&token_id)
						.ok_or(ConvertMessageError::InvalidAsset)?;
					let mut reanchored_asset_loc = asset_loc.clone();
					reanchored_asset_loc
						.reanchor(&GlobalAssetHubLocation::get(), &EthereumUniversalLocation::get())
						.map_err(|_| ConvertMessageError::CannotReanchor)?;
					let asset: XcmAsset = (reanchored_asset_loc, *value).into();
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

		// Add the XCM sent in the message to the end of the xcm instruction
		instructions.extend(message_xcm.0);

		let appendix = vec![
			RefundSurplus,
			// Refund excess fees to the claimer, if present, otherwise the relayer
			DepositAsset {
				assets: Wild(AllOf { id: AssetId(fee_asset.into()), fun: WildFungible }),
				beneficiary: refund_surplus_to,
			},
		];

		instructions.extend(appendix);

		Ok((instructions.into(), message.relayer_fee))
	}
}

#[cfg(test)]
mod tests {
	use crate::inbound::v2::{
		Asset::{ForeignTokenERC20, NativeTokenERC20},
		ConvertMessage, ConvertMessageError, Message, MessageToXcm, XcmAsset,
	};
	use codec::Encode;
	use frame_support::{assert_err, assert_ok, parameter_types};
	use hex_literal::hex;
	use snowbridge_core::TokenId;
	use sp_core::{H160, H256};
	use sp_runtime::traits::MaybeEquivalence;
	use xcm::{opaque::latest::WESTEND_GENESIS_HASH, prelude::*};
	const GATEWAY_ADDRESS: [u8; 20] = hex!["eda338e4dc46038493b885327842fd3e301cab39"];
	const WETH_ADDRESS: [u8; 20] = hex!["fff9976782d46cc05630d1f6ebab18b2324d6b14"];

	parameter_types! {
		pub const EthereumNetwork: xcm::v5::NetworkId = xcm::v5::NetworkId::Ethereum { chain_id: 11155111 };
		pub const GatewayAddress: H160 = H160(GATEWAY_ADDRESS);
		pub const WethAddress: H160 = H160(WETH_ADDRESS);
		pub const InboundQueuePalletInstance: u8 = 84;
		pub AssetHubLocation: InteriorLocation = Parachain(1000).into();
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
		let origin_account =
			Location::new(0, [AccountId32 { network: None, id: H256::random().into() }]);
		let origin: H160 = hex!("29e3b139f4393adda86303fcdaa35f60bb7092bf").into();
		let native_token_id: H160 = hex!("5615deb798bb3e4dfa0139dfa1b3d433cc23b72f").into();
		let foreign_token_id: H256 =
			hex!("37a6c666da38711a963d938eafdd09314fd3f95a96a3baffb55f26560f4ecdd8").into();
		let beneficiary =
			hex!("908783d8cd24c9e02cee1d26ab9c46d458621ad0150b626c536a40b9df3f09c6").into();
		let message_id: H256 =
			hex!("8b69c7e376e28114618e829a7ec768dbda28357d359ba417a3bd79b11215059d").into();
		let token_value = 3_000_000_000_000u128;
		let assets = vec![
			NativeTokenERC20 { token_id: native_token_id, value: token_value },
			ForeignTokenERC20 { token_id: foreign_token_id, value: token_value },
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
			InboundQueuePalletInstance,
			MockTokenIdConvert,
			WethAddress,
			GatewayAddress,
			UniversalLocation,
			AssetHubFromEthereum,
		>::convert(message, origin_account);

		assert_ok!(result.clone());

		let (xcm, _) = result.unwrap();

		let mut instructions = xcm.into_iter();

		let mut asset_claimer_found = false;
		let mut pay_fees_found = false;
		let mut descend_origin_found = 0;
		let mut reserve_deposited_found = 0;
		let mut withdraw_assets_found = 0;
		while let Some(instruction) = instructions.next() {
			if let SetAssetClaimer { ref location } = instruction {
				assert_eq!(Location::new(0, [claimer_account]), location.clone());
				asset_claimer_found = true;
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
				let fee_asset = Location::new(
					2,
					[
						GlobalConsensus(EthereumNetwork::get()),
						AccountKey20 { network: None, key: WethAddress::get().into() },
					],
				);
				assert_eq!(asset.id, AssetId(fee_asset));
				assert_eq!(asset.fun, Fungible(execution_fee));
				pay_fees_found = true;
			}
			if let ReserveAssetDeposited(ref reserve_assets) = instruction {
				reserve_deposited_found = reserve_deposited_found + 1;
				if reserve_deposited_found == 1 {
					let fee_asset = Location::new(
						2,
						[
							GlobalConsensus(EthereumNetwork::get()),
							AccountKey20 { network: None, key: WethAddress::get().into() },
						],
					);
					let fee: XcmAsset = (fee_asset, execution_fee).into();
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
					let token: XcmAsset = (token_asset, token_value).into();
					let token_assets: Assets = token.into();
					assert_eq!(token_assets, reserve_assets.clone());
				}
			}
			if let WithdrawAsset(ref withdraw_assets) = instruction {
				withdraw_assets_found = withdraw_assets_found + 1;
				let token_asset = Location::new(2, Here);
				let token: XcmAsset = (token_asset, token_value).into();
				let token_assets: Assets = token.into();
				assert_eq!(token_assets, withdraw_assets.clone());
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
			NativeTokenERC20 { token_id: native_token_id, value: token_value },
			ForeignTokenERC20 { token_id: foreign_token_id, value: token_value },
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
			InboundQueuePalletInstance,
			MockTokenIdConvert,
			WethAddress,
			GatewayAddress,
			UniversalLocation,
			AssetHubFromEthereum,
		>::convert(message, origin_account);

		assert_ok!(result.clone());

		let (xcm, _) = result.unwrap();

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
		let assets = vec![ForeignTokenERC20 { token_id, value: token_value }];
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
			InboundQueuePalletInstance,
			MockFailedTokenConvert,
			WethAddress,
			GatewayAddress,
			UniversalLocation,
			AssetHubFromEthereum,
		>::convert(message, origin_account);

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
		let assets = vec![ForeignTokenERC20 { token_id, value: token_value }];
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
			InboundQueuePalletInstance,
			MockTokenIdConvert,
			WethAddress,
			GatewayAddress,
			UniversalLocation,
			AssetHubFromEthereum,
		>::convert(message, origin_account.clone());

		// Invalid claimer does not break the message conversion
		assert_ok!(result.clone());

		let (xcm, _) = result.unwrap();

		let mut result_instructions = xcm.clone().into_iter();

		let mut found = false;
		while let Some(instruction) = result_instructions.next() {
			if let SetAssetClaimer { .. } = instruction {
				found = true;
				break;
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

		let fee_asset = Location::new(
			2,
			[
				GlobalConsensus(EthereumNetwork::get()),
				AccountKey20 { network: None, key: WethAddress::get().into() },
			],
		);
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
		let origin_account =
			Location::new(0, [AccountId32 { network: None, id: H256::random().into() }]);
		let origin: H160 = hex!("29e3b139f4393adda86303fcdaa35f60bb7092bf").into();
		let token_id: H256 =
			hex!("37a6c666da38711a963d938eafdd09314fd3f95a96a3baffb55f26560f4ecdd8").into();
		let token_value = 3_000_000_000_000u128;
		let assets = vec![ForeignTokenERC20 { token_id, value: token_value }];
		// invalid xcm
		let versioned_xcm = hex!("8b69c7e376e28114618e829a7ec7").to_vec();
		let claimer_account = AccountId32 { network: None, id: H256::random().into() };
		let claimer: Option<Vec<u8>> = Some(claimer_account.clone().encode());
		let value = 6_000_000_000_000u128;
		let execution_fee = 1_000_000_000_000u128;
		let relayer_fee = 5_000_000_000_000u128;

		let message = Message {
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
			InboundQueuePalletInstance,
			MockTokenIdConvert,
			WethAddress,
			GatewayAddress,
			UniversalLocation,
			AssetHubFromEthereum,
		>::convert(message, origin_account.clone());

		// Invalid xcm does not break the message, allowing funds to be trapped on AH.
		assert_ok!(result.clone());
	}
}
