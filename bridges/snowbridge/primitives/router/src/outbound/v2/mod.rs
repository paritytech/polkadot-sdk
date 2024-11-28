// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Converts XCM messages into simpler commands that can be processed by the Gateway contract

pub mod convert;
use convert::XcmConverter;

use codec::{Decode, Encode};
use frame_support::{
	ensure,
	traits::{Contains, Get, ProcessMessageError},
};
use snowbridge_core::{outbound::v2::SendMessage, TokenId};
use sp_core::{H160, H256};
use sp_runtime::traits::MaybeEquivalence;
use sp_std::{marker::PhantomData, ops::ControlFlow, prelude::*};
use xcm::prelude::*;
use xcm_builder::{CreateMatcher, ExporterFor, MatchXcm};
use xcm_executor::traits::{ConvertLocation, ExportXcm};

pub const TARGET: &'static str = "xcm::ethereum_blob_exporter::v2";

pub struct EthereumBlobExporter<
	UniversalLocation,
	EthereumNetwork,
	OutboundQueue,
	AgentHashedDescription,
	ConvertAssetId,
	WETHAddress,
>(
	PhantomData<(
		UniversalLocation,
		EthereumNetwork,
		OutboundQueue,
		AgentHashedDescription,
		ConvertAssetId,
		WETHAddress,
	)>,
);

impl<
		UniversalLocation,
		EthereumNetwork,
		OutboundQueue,
		AgentHashedDescription,
		ConvertAssetId,
		WETHAddress,
	> ExportXcm
	for EthereumBlobExporter<
		UniversalLocation,
		EthereumNetwork,
		OutboundQueue,
		AgentHashedDescription,
		ConvertAssetId,
		WETHAddress,
	>
where
	UniversalLocation: Get<InteriorLocation>,
	EthereumNetwork: Get<NetworkId>,
	OutboundQueue: SendMessage<Balance = u128>,
	AgentHashedDescription: ConvertLocation<H256>,
	ConvertAssetId: MaybeEquivalence<TokenId, Location>,
	WETHAddress: Get<H160>,
{
	type Ticket = (Vec<u8>, XcmHash);

	fn validate(
		network: NetworkId,
		_channel: u32,
		universal_source: &mut Option<InteriorLocation>,
		destination: &mut Option<InteriorLocation>,
		message: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		log::debug!(target: TARGET, "message route through bridge {message:?}.");

		let expected_network = EthereumNetwork::get();
		let universal_location = UniversalLocation::get();

		if network != expected_network {
			log::trace!(target: TARGET, "skipped due to unmatched bridge network {network:?}.");
			return Err(SendError::NotApplicable)
		}

		// Cloning destination to avoid modifying the value so subsequent exporters can use it.
		let dest = destination.clone().ok_or(SendError::MissingArgument)?;
		if dest != Here {
			log::trace!(target: TARGET, "skipped due to unmatched remote destination {dest:?}.");
			return Err(SendError::NotApplicable)
		}

		// Cloning universal_source to avoid modifying the value so subsequent exporters can use it.
		let (local_net, _) = universal_source.clone()
            .ok_or_else(|| {
                log::error!(target: TARGET, "universal source not provided.");
                SendError::MissingArgument
            })?
            .split_global()
            .map_err(|()| {
                log::error!(target: TARGET, "could not get global consensus from universal source '{universal_source:?}'.");
                SendError::NotApplicable
            })?;

		if Ok(local_net) != universal_location.global_consensus() {
			log::trace!(target: TARGET, "skipped due to unmatched relay network {local_net:?}.");
			return Err(SendError::NotApplicable)
		}

		let message = message.clone().ok_or_else(|| {
			log::error!(target: TARGET, "xcm message not provided.");
			SendError::MissingArgument
		})?;

		// Inspect AliasOrigin as V2 message
		let mut instructions = message.clone().0;
		let result = instructions.matcher().match_next_inst_while(
			|_| true,
			|inst| {
				return match inst {
					AliasOrigin(..) => Err(ProcessMessageError::Yield),
					_ => Ok(ControlFlow::Continue(())),
				}
			},
		);
		ensure!(result.is_err(), SendError::NotApplicable);

		let mut converter =
			XcmConverter::<ConvertAssetId, WETHAddress, ()>::new(&message, expected_network);
		let message = converter.convert().map_err(|err| {
			log::error!(target: TARGET, "unroutable due to pattern matching error '{err:?}'.");
			SendError::Unroutable
		})?;

		// validate the message
		let (ticket, _) = OutboundQueue::validate(&message).map_err(|err| {
			log::error!(target: TARGET, "OutboundQueue validation of message failed. {err:?}");
			SendError::Unroutable
		})?;

		Ok(((ticket.encode(), XcmHash::from(message.id)), Assets::default()))
	}

	fn deliver(blob: (Vec<u8>, XcmHash)) -> Result<XcmHash, SendError> {
		let ticket: OutboundQueue::Ticket = OutboundQueue::Ticket::decode(&mut blob.0.as_ref())
			.map_err(|_| {
				log::trace!(target: TARGET, "undeliverable due to decoding error");
				SendError::NotApplicable
			})?;

		let message_id = OutboundQueue::deliver(ticket).map_err(|_| {
			log::error!(target: TARGET, "OutboundQueue submit of message failed");
			SendError::Transport("other transport error")
		})?;

		log::info!(target: TARGET, "message delivered {message_id:#?}.");
		Ok(message_id.into())
	}
}

/// An adapter for the implementation of `ExporterFor`, which attempts to find the
/// `(bridge_location, payment)` for the requested `network` and `remote_location` and `xcm`
/// in the provided `T` table containing various exporters.
pub struct XcmFilterExporter<T, M>(core::marker::PhantomData<(T, M)>);
impl<T: ExporterFor, M: Contains<Xcm<()>>> ExporterFor for XcmFilterExporter<T, M> {
	fn exporter_for(
		network: &NetworkId,
		remote_location: &InteriorLocation,
		xcm: &Xcm<()>,
	) -> Option<(Location, Option<Asset>)> {
		// check the XCM
		if !M::contains(xcm) {
			return None
		}
		// check `network` and `remote_location`
		T::exporter_for(network, remote_location, xcm)
	}
}

/// Xcm for SnowbridgeV2 which requires XCMV5
pub struct XcmForSnowbridgeV2;
impl Contains<Xcm<()>> for XcmForSnowbridgeV2 {
	fn contains(xcm: &Xcm<()>) -> bool {
		let mut instructions = xcm.clone().0;
		let result = instructions.matcher().match_next_inst_while(
			|_| true,
			|inst| {
				return match inst {
					AliasOrigin(..) => Err(ProcessMessageError::Yield),
					_ => Ok(ControlFlow::Continue(())),
				}
			},
		);
		result.is_err()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use frame_support::parameter_types;
	use hex_literal::hex;
	use snowbridge_core::{
		outbound::{v2::Message, SendError, SendMessageFeeProvider},
		AgentIdOf,
	};
	use sp_std::default::Default;
	use xcm::{latest::WESTEND_GENESIS_HASH, prelude::SendError as XcmSendError};

	parameter_types! {
		const MaxMessageSize: u32 = u32::MAX;
		const RelayNetwork: NetworkId = Polkadot;
		UniversalLocation: InteriorLocation = [GlobalConsensus(RelayNetwork::get()), Parachain(1013)].into();
		pub const BridgedNetwork: NetworkId =  Ethereum{ chain_id: 1 };
		pub const NonBridgedNetwork: NetworkId =  Ethereum{ chain_id: 2 };
		pub WETHAddress: H160 = H160(hex_literal::hex!("c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"));
	}

	struct MockOkOutboundQueue;
	impl SendMessage for MockOkOutboundQueue {
		type Ticket = ();

		type Balance = u128;

		fn validate(_: &Message) -> Result<(Self::Ticket, Self::Balance), SendError> {
			Ok(((), 1_u128))
		}

		fn deliver(_: Self::Ticket) -> Result<H256, SendError> {
			Ok(H256::zero())
		}
	}

	impl SendMessageFeeProvider for MockOkOutboundQueue {
		type Balance = u128;

		fn local_fee() -> Self::Balance {
			1
		}
	}
	struct MockErrOutboundQueue;
	impl SendMessage for MockErrOutboundQueue {
		type Ticket = ();

		type Balance = u128;

		fn validate(_: &Message) -> Result<(Self::Ticket, Self::Balance), SendError> {
			Err(SendError::MessageTooLarge)
		}

		fn deliver(_: Self::Ticket) -> Result<H256, SendError> {
			Err(SendError::MessageTooLarge)
		}
	}

	impl SendMessageFeeProvider for MockErrOutboundQueue {
		type Balance = u128;

		fn local_fee() -> Self::Balance {
			1
		}
	}

	pub struct MockTokenIdConvert;
	impl MaybeEquivalence<TokenId, Location> for MockTokenIdConvert {
		fn convert(_id: &TokenId) -> Option<Location> {
			Some(Location::new(1, [GlobalConsensus(ByGenesis(WESTEND_GENESIS_HASH))]))
		}
		fn convert_back(_loc: &Location) -> Option<TokenId> {
			None
		}
	}

	#[test]
	fn exporter_validate_with_unknown_network_yields_not_applicable() {
		let network = Ethereum { chain_id: 1337 };
		let channel: u32 = 0;
		let mut universal_source: Option<InteriorLocation> = None;
		let mut destination: Option<InteriorLocation> = None;
		let mut message: Option<Xcm<()>> = None;

		let result =
			EthereumBlobExporter::<
				UniversalLocation,
				BridgedNetwork,
				MockOkOutboundQueue,
				AgentIdOf,
				MockTokenIdConvert,
				WETHAddress,
			>::validate(network, channel, &mut universal_source, &mut destination, &mut message);
		assert_eq!(result, Err(XcmSendError::NotApplicable));
	}

	#[test]
	fn exporter_validate_with_invalid_destination_yields_missing_argument() {
		let network = BridgedNetwork::get();
		let channel: u32 = 0;
		let mut universal_source: Option<InteriorLocation> = None;
		let mut destination: Option<InteriorLocation> = None;
		let mut message: Option<Xcm<()>> = None;

		let result =
			EthereumBlobExporter::<
				UniversalLocation,
				BridgedNetwork,
				MockOkOutboundQueue,
				AgentIdOf,
				MockTokenIdConvert,
				WETHAddress,
			>::validate(network, channel, &mut universal_source, &mut destination, &mut message);
		assert_eq!(result, Err(XcmSendError::MissingArgument));
	}

	#[test]
	fn exporter_validate_with_x8_destination_yields_not_applicable() {
		let network = BridgedNetwork::get();
		let channel: u32 = 0;
		let mut universal_source: Option<InteriorLocation> = None;
		let mut destination: Option<InteriorLocation> = Some(
			[
				OnlyChild, OnlyChild, OnlyChild, OnlyChild, OnlyChild, OnlyChild, OnlyChild,
				OnlyChild,
			]
			.into(),
		);
		let mut message: Option<Xcm<()>> = None;

		let result =
			EthereumBlobExporter::<
				UniversalLocation,
				BridgedNetwork,
				MockOkOutboundQueue,
				AgentIdOf,
				MockTokenIdConvert,
				WETHAddress,
			>::validate(network, channel, &mut universal_source, &mut destination, &mut message);
		assert_eq!(result, Err(XcmSendError::NotApplicable));
	}

	#[test]
	fn exporter_validate_without_universal_source_yields_missing_argument() {
		let network = BridgedNetwork::get();
		let channel: u32 = 0;
		let mut universal_source: Option<InteriorLocation> = None;
		let mut destination: Option<InteriorLocation> = Here.into();
		let mut message: Option<Xcm<()>> = None;

		let result =
			EthereumBlobExporter::<
				UniversalLocation,
				BridgedNetwork,
				MockOkOutboundQueue,
				AgentIdOf,
				MockTokenIdConvert,
				WETHAddress,
			>::validate(network, channel, &mut universal_source, &mut destination, &mut message);
		assert_eq!(result, Err(XcmSendError::MissingArgument));
	}

	#[test]
	fn exporter_validate_without_global_universal_location_yields_not_applicable() {
		let network = BridgedNetwork::get();
		let channel: u32 = 0;
		let mut universal_source: Option<InteriorLocation> = Here.into();
		let mut destination: Option<InteriorLocation> = Here.into();
		let mut message: Option<Xcm<()>> = None;

		let result =
			EthereumBlobExporter::<
				UniversalLocation,
				BridgedNetwork,
				MockOkOutboundQueue,
				AgentIdOf,
				MockTokenIdConvert,
				WETHAddress,
			>::validate(network, channel, &mut universal_source, &mut destination, &mut message);
		assert_eq!(result, Err(XcmSendError::NotApplicable));
	}

	#[test]
	fn exporter_validate_without_global_bridge_location_yields_not_applicable() {
		let network = NonBridgedNetwork::get();
		let channel: u32 = 0;
		let mut universal_source: Option<InteriorLocation> = Here.into();
		let mut destination: Option<InteriorLocation> = Here.into();
		let mut message: Option<Xcm<()>> = None;

		let result =
			EthereumBlobExporter::<
				UniversalLocation,
				BridgedNetwork,
				MockOkOutboundQueue,
				AgentIdOf,
				MockTokenIdConvert,
				WETHAddress,
			>::validate(network, channel, &mut universal_source, &mut destination, &mut message);
		assert_eq!(result, Err(XcmSendError::NotApplicable));
	}

	#[test]
	fn exporter_validate_with_remote_universal_source_yields_not_applicable() {
		let network = BridgedNetwork::get();
		let channel: u32 = 0;
		let mut universal_source: Option<InteriorLocation> =
			Some([GlobalConsensus(Kusama), Parachain(1000)].into());
		let mut destination: Option<InteriorLocation> = Here.into();
		let mut message: Option<Xcm<()>> = None;

		let result =
			EthereumBlobExporter::<
				UniversalLocation,
				BridgedNetwork,
				MockOkOutboundQueue,
				AgentIdOf,
				MockTokenIdConvert,
				WETHAddress,
			>::validate(network, channel, &mut universal_source, &mut destination, &mut message);
		assert_eq!(result, Err(XcmSendError::NotApplicable));
	}

	#[test]
	fn exporter_validate_without_para_id_in_source_yields_not_applicable() {
		let network = BridgedNetwork::get();
		let channel: u32 = 0;
		let mut universal_source: Option<InteriorLocation> = Some(GlobalConsensus(Polkadot).into());
		let mut destination: Option<InteriorLocation> = Here.into();
		let mut message: Option<Xcm<()>> = None;

		let result =
			EthereumBlobExporter::<
				UniversalLocation,
				BridgedNetwork,
				MockOkOutboundQueue,
				AgentIdOf,
				MockTokenIdConvert,
				WETHAddress,
			>::validate(network, channel, &mut universal_source, &mut destination, &mut message);
		assert_eq!(result, Err(XcmSendError::MissingArgument));
	}

	#[test]
	fn exporter_validate_complex_para_id_in_source_yields_not_applicable() {
		let network = BridgedNetwork::get();
		let channel: u32 = 0;
		let mut universal_source: Option<InteriorLocation> =
			Some([GlobalConsensus(Polkadot), Parachain(1000), PalletInstance(12)].into());
		let mut destination: Option<InteriorLocation> = Here.into();
		let mut message: Option<Xcm<()>> = None;

		let result =
			EthereumBlobExporter::<
				UniversalLocation,
				BridgedNetwork,
				MockOkOutboundQueue,
				AgentIdOf,
				MockTokenIdConvert,
				WETHAddress,
			>::validate(network, channel, &mut universal_source, &mut destination, &mut message);
		assert_eq!(result, Err(XcmSendError::MissingArgument));
	}

	#[test]
	fn exporter_validate_without_xcm_message_yields_missing_argument() {
		let network = BridgedNetwork::get();
		let channel: u32 = 0;
		let mut universal_source: Option<InteriorLocation> =
			Some([GlobalConsensus(Polkadot), Parachain(1000)].into());
		let mut destination: Option<InteriorLocation> = Here.into();
		let mut message: Option<Xcm<()>> = None;

		let result =
			EthereumBlobExporter::<
				UniversalLocation,
				BridgedNetwork,
				MockOkOutboundQueue,
				AgentIdOf,
				MockTokenIdConvert,
				WETHAddress,
			>::validate(network, channel, &mut universal_source, &mut destination, &mut message);
		assert_eq!(result, Err(XcmSendError::MissingArgument));
	}

	#[test]
	fn exporter_validate_with_max_target_fee_yields_unroutable() {
		let network = BridgedNetwork::get();
		let mut destination: Option<InteriorLocation> = Here.into();

		let mut universal_source: Option<InteriorLocation> =
			Some([GlobalConsensus(Polkadot), Parachain(1000)].into());

		let token_address: [u8; 20] = hex!("1000000000000000000000000000000000000000");
		let beneficiary_address: [u8; 20] = hex!("2000000000000000000000000000000000000000");

		let channel: u32 = 0;
		let fee = Asset { id: AssetId(Here.into()), fun: Fungible(1000) };
		let fees: Assets = vec![fee.clone()].into();
		let assets: Assets = vec![Asset {
			id: AssetId(AccountKey20 { network: None, key: token_address }.into()),
			fun: Fungible(1000),
		}]
		.into();
		let filter: AssetFilter = assets.clone().into();

		let mut message: Option<Xcm<()>> = Some(
			vec![
				WithdrawAsset(fees),
				BuyExecution { fees: fee.clone(), weight_limit: Unlimited },
				ExpectAsset(fee.into()),
				WithdrawAsset(assets),
				DepositAsset {
					assets: filter,
					beneficiary: AccountKey20 { network: Some(network), key: beneficiary_address }
						.into(),
				},
				SetTopic([0; 32]),
			]
			.into(),
		);

		let result =
			EthereumBlobExporter::<
				UniversalLocation,
				BridgedNetwork,
				MockOkOutboundQueue,
				AgentIdOf,
				MockTokenIdConvert,
				WETHAddress,
			>::validate(network, channel, &mut universal_source, &mut destination, &mut message);

		assert_eq!(result, Err(XcmSendError::NotApplicable));
	}

	#[test]
	fn exporter_validate_with_unparsable_xcm_yields_unroutable() {
		let network = BridgedNetwork::get();
		let mut destination: Option<InteriorLocation> = Here.into();

		let mut universal_source: Option<InteriorLocation> =
			Some([GlobalConsensus(Polkadot), Parachain(1000)].into());

		let channel: u32 = 0;
		let fee = Asset { id: AssetId(Here.into()), fun: Fungible(1000) };
		let fees: Assets = vec![fee.clone()].into();

		let mut message: Option<Xcm<()>> = Some(
			vec![WithdrawAsset(fees), BuyExecution { fees: fee, weight_limit: Unlimited }].into(),
		);

		let result =
			EthereumBlobExporter::<
				UniversalLocation,
				BridgedNetwork,
				MockOkOutboundQueue,
				AgentIdOf,
				MockTokenIdConvert,
				WETHAddress,
			>::validate(network, channel, &mut universal_source, &mut destination, &mut message);

		assert_eq!(result, Err(XcmSendError::NotApplicable));
	}

	#[test]
	fn exporter_validate_xcm_success_case_1() {
		let network = BridgedNetwork::get();
		let mut destination: Option<InteriorLocation> = Here.into();

		let mut universal_source: Option<InteriorLocation> =
			Some([GlobalConsensus(Polkadot), Parachain(1000)].into());

		let token_address: [u8; 20] = hex!("1000000000000000000000000000000000000000");
		let beneficiary_address: [u8; 20] = hex!("2000000000000000000000000000000000000000");

		let channel: u32 = 0;
		let assets: Assets = vec![Asset {
			id: AssetId([AccountKey20 { network: None, key: token_address }].into()),
			fun: Fungible(1000),
		}]
		.into();
		let fee_asset: Asset = Asset {
			id: AssetId([AccountKey20 { network: None, key: WETHAddress::get().0 }].into()),
			fun: Fungible(1000),
		}
		.into();
		let filter: AssetFilter = assets.clone().into();

		let mut message: Option<Xcm<()>> = Some(
			vec![
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
			.into(),
		);

		let result =
			EthereumBlobExporter::<
				UniversalLocation,
				BridgedNetwork,
				MockOkOutboundQueue,
				AgentIdOf,
				MockTokenIdConvert,
				WETHAddress,
			>::validate(network, channel, &mut universal_source, &mut destination, &mut message);

		assert!(result.is_ok());
	}

	#[test]
	fn exporter_deliver_with_submit_failure_yields_unroutable() {
		let result = EthereumBlobExporter::<
			UniversalLocation,
			BridgedNetwork,
			MockErrOutboundQueue,
			AgentIdOf,
			MockTokenIdConvert,
			WETHAddress,
		>::deliver((hex!("deadbeef").to_vec(), XcmHash::default()));
		assert_eq!(result, Err(XcmSendError::Transport("other transport error")))
	}

	#[test]
	fn exporter_validate_with_invalid_dest_does_not_alter_destination() {
		let network = BridgedNetwork::get();
		let destination: InteriorLocation = Parachain(1000).into();

		let universal_source: InteriorLocation =
			[GlobalConsensus(Polkadot), Parachain(1000)].into();

		let token_address: [u8; 20] = hex!("1000000000000000000000000000000000000000");
		let beneficiary_address: [u8; 20] = hex!("2000000000000000000000000000000000000000");

		let channel: u32 = 0;
		let assets: Assets = vec![Asset {
			id: AssetId([AccountKey20 { network: None, key: token_address }].into()),
			fun: Fungible(1000),
		}]
		.into();
		let fee = assets.clone().get(0).unwrap().clone();
		let filter: AssetFilter = assets.clone().into();
		let msg: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
			ClearOrigin,
			BuyExecution { fees: fee, weight_limit: Unlimited },
			DepositAsset {
				assets: filter,
				beneficiary: AccountKey20 { network: None, key: beneficiary_address }.into(),
			},
			SetTopic([0; 32]),
		]
		.into();
		let mut msg_wrapper: Option<Xcm<()>> = Some(msg.clone());
		let mut dest_wrapper = Some(destination.clone());
		let mut universal_source_wrapper = Some(universal_source.clone());

		let result = EthereumBlobExporter::<
			UniversalLocation,
			BridgedNetwork,
			MockOkOutboundQueue,
			AgentIdOf,
			MockTokenIdConvert,
			WETHAddress,
		>::validate(
			network,
			channel,
			&mut universal_source_wrapper,
			&mut dest_wrapper,
			&mut msg_wrapper,
		);

		assert_eq!(result, Err(XcmSendError::NotApplicable));

		// ensure mutable variables are not changed
		assert_eq!(Some(destination), dest_wrapper);
		assert_eq!(Some(msg), msg_wrapper);
		assert_eq!(Some(universal_source), universal_source_wrapper);
	}

	#[test]
	fn exporter_validate_with_invalid_universal_source_does_not_alter_universal_source() {
		let network = BridgedNetwork::get();
		let destination: InteriorLocation = Here.into();

		let universal_source: InteriorLocation =
			[GlobalConsensus(ByGenesis(WESTEND_GENESIS_HASH)), Parachain(1000)].into();

		let token_address: [u8; 20] = hex!("1000000000000000000000000000000000000000");
		let beneficiary_address: [u8; 20] = hex!("2000000000000000000000000000000000000000");

		let channel: u32 = 0;
		let assets: Assets = vec![Asset {
			id: AssetId([AccountKey20 { network: None, key: token_address }].into()),
			fun: Fungible(1000),
		}]
		.into();
		let fee = assets.clone().get(0).unwrap().clone();
		let filter: AssetFilter = assets.clone().into();
		let msg: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
			ClearOrigin,
			BuyExecution { fees: fee, weight_limit: Unlimited },
			DepositAsset {
				assets: filter,
				beneficiary: AccountKey20 { network: None, key: beneficiary_address }.into(),
			},
			SetTopic([0; 32]),
		]
		.into();
		let mut msg_wrapper: Option<Xcm<()>> = Some(msg.clone());
		let mut dest_wrapper = Some(destination.clone());
		let mut universal_source_wrapper = Some(universal_source.clone());

		let result = EthereumBlobExporter::<
			UniversalLocation,
			BridgedNetwork,
			MockOkOutboundQueue,
			AgentIdOf,
			MockTokenIdConvert,
			WETHAddress,
		>::validate(
			network,
			channel,
			&mut universal_source_wrapper,
			&mut dest_wrapper,
			&mut msg_wrapper,
		);

		assert_eq!(result, Err(XcmSendError::NotApplicable));

		// ensure mutable variables are not changed
		assert_eq!(Some(destination), dest_wrapper);
		assert_eq!(Some(msg), msg_wrapper);
		assert_eq!(Some(universal_source), universal_source_wrapper);
	}
}
