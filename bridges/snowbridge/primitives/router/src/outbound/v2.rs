// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Converts XCM messages into simpler commands that can be processed by the Gateway contract

use codec::{Decode, Encode};
use core::slice::Iter;
use sp_std::ops::ControlFlow;

use frame_support::{
	ensure,
	traits::{Contains, Get, ProcessMessageError},
	BoundedVec,
};
use snowbridge_core::{
	outbound::v2::{Command, Message, SendMessage},
	AgentId, TokenId, TokenIdOf,
};
use sp_core::{H160, H256};
use sp_runtime::traits::{BlakeTwo256, Hash, MaybeEquivalence};
use sp_std::{iter::Peekable, marker::PhantomData, prelude::*};
use xcm::prelude::*;
use xcm_builder::{CreateMatcher, ExporterFor, MatchXcm};
use xcm_executor::traits::{ConvertLocation, ExportXcm};

const TARGET: &'static str = "xcm::ethereum_blob_exporter::v2";

pub struct EthereumBlobExporter<
	UniversalLocation,
	EthereumNetwork,
	OutboundQueue,
	AgentHashedDescription,
	ConvertAssetId,
>(
	PhantomData<(
		UniversalLocation,
		EthereumNetwork,
		OutboundQueue,
		AgentHashedDescription,
		ConvertAssetId,
	)>,
);

impl<UniversalLocation, EthereumNetwork, OutboundQueue, AgentHashedDescription, ConvertAssetId>
	ExportXcm
	for EthereumBlobExporter<
		UniversalLocation,
		EthereumNetwork,
		OutboundQueue,
		AgentHashedDescription,
		ConvertAssetId,
	>
where
	UniversalLocation: Get<InteriorLocation>,
	EthereumNetwork: Get<NetworkId>,
	OutboundQueue: SendMessage<Balance = u128>,
	AgentHashedDescription: ConvertLocation<H256>,
	ConvertAssetId: MaybeEquivalence<TokenId, Location>,
{
	type Ticket = (Vec<u8>, XcmHash);

	fn validate(
		network: NetworkId,
		_channel: u32,
		universal_source: &mut Option<InteriorLocation>,
		destination: &mut Option<InteriorLocation>,
		message: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
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
		let (local_net, local_sub) = universal_source.clone()
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

		let source_location = Location::new(1, local_sub.clone());

		let agent_id = match AgentHashedDescription::convert_location(&source_location) {
			Some(id) => id,
			None => {
				log::error!(target: TARGET, "unroutable due to not being able to create agent id. '{source_location:?}'");
				return Err(SendError::NotApplicable)
			},
		};

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
			XcmConverter::<ConvertAssetId, ()>::new(&message, expected_network, agent_id);
		let message = converter.convert().map_err(|err| {
			log::error!(target: TARGET, "unroutable due to pattern matching error '{err:?}'.");
			SendError::Unroutable
		})?;

		// validate the message
		let (ticket, fee) = OutboundQueue::validate(&message).map_err(|err| {
			log::error!(target: TARGET, "OutboundQueue validation of message failed. {err:?}");
			SendError::Unroutable
		})?;

		// convert fee to Asset
		let fee = Asset::from((Location::parent(), fee.total())).into();

		Ok(((ticket.encode(), XcmHash::from(message.id)), fee))
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

/// Errors that can be thrown to the pattern matching step.
#[derive(PartialEq, Debug)]
enum XcmConverterError {
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
}

macro_rules! match_expression {
	($expression:expr, $(|)? $( $pattern:pat_param )|+ $( if $guard: expr )?, $value:expr $(,)?) => {
		match $expression {
			$( $pattern )|+ $( if $guard )? => Some($value),
			_ => None,
		}
	};
}

struct XcmConverter<'a, ConvertAssetId, Call> {
	iter: Peekable<Iter<'a, Instruction<Call>>>,
	message: Vec<Instruction<Call>>,
	ethereum_network: NetworkId,
	agent_id: AgentId,
	_marker: PhantomData<ConvertAssetId>,
}
impl<'a, ConvertAssetId, Call> XcmConverter<'a, ConvertAssetId, Call>
where
	ConvertAssetId: MaybeEquivalence<TokenId, Location>,
{
	fn new(message: &'a Xcm<Call>, ethereum_network: NetworkId, agent_id: AgentId) -> Self {
		Self {
			message: message.clone().inner().into(),
			iter: message.inner().iter().peekable(),
			ethereum_network,
			agent_id,
			_marker: Default::default(),
		}
	}

	fn convert(&mut self) -> Result<Message, XcmConverterError> {
		let result = match self.jump_to() {
			// PNA
			Ok(ReserveAssetDeposited { .. }) => self.send_native_tokens_message(),
			// ENA
			Ok(WithdrawAsset { .. }) => self.send_tokens_message(),
			Err(e) => Err(e),
			_ => return Err(XcmConverterError::UnexpectedInstruction),
		}?;

		// All xcm instructions must be consumed before exit.
		if self.next().is_ok() {
			return Err(XcmConverterError::EndOfXcmMessageExpected)
		}

		Ok(result)
	}

	/// Convert the xcm for Ethereum-native token from AH into the Message which will be executed
	/// on Ethereum Gateway contract, we expect an input of the form:
	/// # WithdrawAsset(WETH_FEE)
	/// # PayFees(WETH_FEE)
	/// # WithdrawAsset(ENA)
	/// # AliasOrigin(Origin)
	/// # DepositAsset(ENA)
	/// # SetTopic
	fn send_tokens_message(&mut self) -> Result<Message, XcmConverterError> {
		use XcmConverterError::*;

		// Get fee amount
		let fee_amount = self.extract_remote_fee()?;

		// Get the reserve assets from WithdrawAsset.
		let reserve_assets =
			match_expression!(self.next()?, WithdrawAsset(reserve_assets), reserve_assets)
				.ok_or(WithdrawAssetExpected)?;

		// Check AliasOrigin.
		let origin = match_expression!(self.next()?, AliasOrigin(origin), origin)
			.ok_or(AliasOriginExpected)?;

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
		if reserve_assets.len() == 0 {
			return Err(NoReserveAssets)
		}

		// Check the the deposit asset filter matches what was reserved.
		if reserve_assets.inner().iter().any(|asset| !deposit_assets.matches(asset)) {
			return Err(FilterDoesNotConsumeAllAssets)
		}

		// We only support a single asset at a time.
		ensure!(reserve_assets.len() == 1, TooManyAssets);
		let reserve_asset = reserve_assets.get(0).ok_or(AssetResolutionFailed)?;

		// only fungible asset is allowed
		let (token, amount) = match reserve_asset {
			Asset { id: AssetId(inner_location), fun: Fungible(amount) } =>
				match inner_location.unpack() {
					(0, [AccountKey20 { network, key }]) if self.network_matches(network) =>
						Some((H160(*key), *amount)),
					_ => None,
				},
			_ => None,
		}
		.ok_or(AssetResolutionFailed)?;

		// transfer amount must be greater than 0.
		ensure!(amount > 0, ZeroAssetTransfer);

		// ensure SetTopic exists
		let topic_id = match_expression!(self.next()?, SetTopic(id), id).ok_or(SetTopicExpected)?;

		let message = Message {
			id: (*topic_id).into(),
			// Todo: Use DescribeLocation
			origin: BlakeTwo256::hash_of(origin),
			fee: fee_amount,
			commands: BoundedVec::try_from(vec![Command::UnlockNativeToken {
				agent_id: self.agent_id,
				token,
				recipient,
				amount,
			}])
			.map_err(|_| TooManyCommands)?,
		};

		Ok(message)
	}

	fn next(&mut self) -> Result<&'a Instruction<Call>, XcmConverterError> {
		self.iter.next().ok_or(XcmConverterError::UnexpectedEndOfXcm)
	}

	fn network_matches(&self, network: &Option<NetworkId>) -> bool {
		if let Some(network) = network {
			*network == self.ethereum_network
		} else {
			true
		}
	}

	/// Convert the xcm for Polkadot-native token from AH into the Message which will be executed
	/// on Ethereum Gateway contract, we expect an input of the form:
	/// # WithdrawAsset(WETH)
	/// # PayFees(WETH)
	/// # ReserveAssetDeposited(PNA)
	/// # AliasOrigin(Origin)
	/// # DepositAsset(PNA)
	/// # SetTopic
	fn send_native_tokens_message(&mut self) -> Result<Message, XcmConverterError> {
		use XcmConverterError::*;

		// Get fee amount
		let fee_amount = self.extract_remote_fee()?;

		// Get the reserve assets.
		let reserve_assets =
			match_expression!(self.next()?, ReserveAssetDeposited(reserve_assets), reserve_assets)
				.ok_or(ReserveAssetDepositedExpected)?;

		// Check AliasOrigin.
		let origin = match_expression!(self.next()?, AliasOrigin(origin), origin)
			.ok_or(AliasOriginExpected)?;

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
		if reserve_assets.len() == 0 {
			return Err(NoReserveAssets)
		}

		// Check the the deposit asset filter matches what was reserved.
		if reserve_assets.inner().iter().any(|asset| !deposit_assets.matches(asset)) {
			return Err(FilterDoesNotConsumeAllAssets)
		}

		// We only support a single asset at a time.
		ensure!(reserve_assets.len() == 1, TooManyAssets);
		let reserve_asset = reserve_assets.get(0).ok_or(AssetResolutionFailed)?;

		// only fungible asset is allowed
		let (asset_id, amount) = match reserve_asset {
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

		// ensure SetTopic exists
		let topic_id = match_expression!(self.next()?, SetTopic(id), id).ok_or(SetTopicExpected)?;

		let message = Message {
			origin: BlakeTwo256::hash_of(origin),
			fee: fee_amount,
			id: (*topic_id).into(),
			commands: BoundedVec::try_from(vec![Command::MintForeignToken {
				token_id,
				recipient,
				amount,
			}])
			.map_err(|_| TooManyCommands)?,
		};

		Ok(message)
	}

	/// Skip fee instructions and jump to the primary asset instruction
	fn jump_to(&mut self) -> Result<&Instruction<Call>, XcmConverterError> {
		ensure!(self.message.len() > 3, XcmConverterError::UnexpectedEndOfXcm);
		self.message.get(2).ok_or(XcmConverterError::UnexpectedEndOfXcm)
	}

	/// Extract the fee asset item from PayFees(V5)
	fn extract_remote_fee(&mut self) -> Result<u128, XcmConverterError> {
		use XcmConverterError::*;
		let _ = match_expression!(self.next()?, WithdrawAsset(fee), fee)
			.ok_or(WithdrawAssetExpected)?;
		let fee_asset =
			match_expression!(self.next()?, PayFees { asset: fee }, fee).ok_or(InvalidFeeAsset)?;
		// Todo: Validate fee asset is WETH
		let fee_amount = match fee_asset {
			Asset { id: _, fun: Fungible(amount) } => Some(*amount),
			_ => None,
		}
		.ok_or(AssetResolutionFailed)?;
		Ok(fee_amount)
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
	use frame_support::parameter_types;
	use hex_literal::hex;
	use snowbridge_core::{
		outbound::{
			v2::{Fee, SendError},
			SendMessageFeeProvider,
		},
		AgentIdOf,
	};
	use sp_std::default::Default;
	use xcm::prelude::SendError as XcmSendError;

	use super::*;

	parameter_types! {
		const MaxMessageSize: u32 = u32::MAX;
		const RelayNetwork: NetworkId = Polkadot;
		UniversalLocation: InteriorLocation = [GlobalConsensus(RelayNetwork::get()), Parachain(1013)].into();
		const BridgedNetwork: NetworkId =  Ethereum{ chain_id: 1 };
		const NonBridgedNetwork: NetworkId =  Ethereum{ chain_id: 2 };
	}

	struct MockOkOutboundQueue;
	impl SendMessage for MockOkOutboundQueue {
		type Ticket = ();

		fn validate(_: &Message) -> Result<(Self::Ticket, Fee<Self::Balance>), SendError> {
			Ok(((), Fee { local: 1 }))
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

		fn validate(_: &Message) -> Result<(Self::Ticket, Fee<Self::Balance>), SendError> {
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
			Some(Location::new(1, [GlobalConsensus(Westend)]))
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
			>::validate(network, channel, &mut universal_source, &mut destination, &mut message);
		assert_eq!(result, Err(XcmSendError::NotApplicable));
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
			>::validate(network, channel, &mut universal_source, &mut destination, &mut message);
		assert_eq!(result, Err(XcmSendError::NotApplicable));
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
			>::validate(network, channel, &mut universal_source, &mut destination, &mut message);

		assert_eq!(result, Err(XcmSendError::Unroutable));
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
		let fee = assets.clone().get(0).unwrap().clone();
		let filter: AssetFilter = assets.clone().into();

		let mut message: Option<Xcm<()>> = Some(
			vec![
				WithdrawAsset(assets.clone()),
				ClearOrigin,
				BuyExecution { fees: fee.clone(), weight_limit: Unlimited },
				ExpectAsset(fee.into()),
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
		>::deliver((hex!("deadbeef").to_vec(), XcmHash::default()));
		assert_eq!(result, Err(XcmSendError::Transport("other transport error")))
	}

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

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
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
			XcmConverter::<MockTokenIdConvert, ()>::new(&message, network, Default::default());
		let expected_payload = Command::UnlockNativeToken {
			agent_id: Default::default(),
			token: token_address.into(),
			recipient: beneficiary_address.into(),
			amount: 1000,
		};
		let expected_message = Message {
			id: [0; 32].into(),
			origin: H256::zero(),
			fee: 1000,
			commands: BoundedVec::try_from(vec![expected_payload]).unwrap(),
		};
		let result = converter.convert();
		assert_eq!(result, Ok(expected_message));
	}

	#[test]
	fn xcm_converter_convert_without_buy_execution_yields_invalid_fee_asset() {
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
			WithdrawAsset(assets.clone()),
			DepositAsset {
				assets: filter,
				beneficiary: AccountKey20 { network: None, key: beneficiary_address }.into(),
			},
			SetTopic([0; 32]),
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, ()>::new(&message, network, Default::default());
		let result = converter.convert();
		assert_eq!(result.err(), Some(XcmConverterError::InvalidFeeAsset));
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

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
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
			XcmConverter::<MockTokenIdConvert, ()>::new(&message, network, Default::default());
		let expected_payload = Command::UnlockNativeToken {
			agent_id: Default::default(),
			token: token_address.into(),
			recipient: beneficiary_address.into(),
			amount: 1000,
		};
		let expected_message = Message {
			id: [0; 32].into(),
			origin: H256::zero(),
			fee: 1000,
			commands: BoundedVec::try_from(vec![expected_payload]).unwrap(),
		};
		let result = converter.convert();
		assert_eq!(result, Ok(expected_message));
	}

	#[test]
	fn xcm_converter_convert_with_fees_less_than_reserve_yields_success() {
		let network = BridgedNetwork::get();

		let token_address: [u8; 20] = hex!("1000000000000000000000000000000000000000");
		let beneficiary_address: [u8; 20] = hex!("2000000000000000000000000000000000000000");

		let asset_location: Location = [AccountKey20 { network: None, key: token_address }].into();
		let fee_asset = Asset { id: AssetId(asset_location.clone()), fun: Fungible(500) };

		let assets: Assets =
			vec![Asset { id: AssetId(asset_location), fun: Fungible(1000) }].into();

		let filter: AssetFilter = assets.clone().into();

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
			ClearOrigin,
			BuyExecution { fees: fee_asset.clone(), weight_limit: Unlimited },
			ExpectAsset(fee_asset.into()),
			DepositAsset {
				assets: filter,
				beneficiary: AccountKey20 { network: None, key: beneficiary_address }.into(),
			},
			SetTopic([0; 32]),
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, ()>::new(&message, network, Default::default());
		let expected_payload = Command::UnlockNativeToken {
			agent_id: Default::default(),
			token: token_address.into(),
			recipient: beneficiary_address.into(),
			amount: 1000,
		};
		let expected_message = Message {
			id: [0; 32].into(),
			origin: H256::zero(),
			fee: 500,
			commands: BoundedVec::try_from(vec![expected_payload]).unwrap(),
		};
		let result = converter.convert();
		assert_eq!(result, Ok(expected_message));
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

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
			BuyExecution { fees: assets.get(0).unwrap().clone(), weight_limit: Unlimited },
			DepositAsset {
				assets: filter,
				beneficiary: AccountKey20 { network: None, key: beneficiary_address }.into(),
			},
			ClearTopic,
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, ()>::new(&message, network, Default::default());
		let result = converter.convert();
		assert_eq!(result.err(), Some(XcmConverterError::SetTopicExpected));
	}

	#[test]
	fn xcm_converter_convert_with_partial_message_yields_unexpected_end_of_xcm() {
		let network = BridgedNetwork::get();

		let token_address: [u8; 20] = hex!("1000000000000000000000000000000000000000");
		let assets: Assets = vec![Asset {
			id: AssetId([AccountKey20 { network: None, key: token_address }].into()),
			fun: Fungible(1000),
		}]
		.into();
		let message: Xcm<()> = vec![WithdrawAsset(assets)].into();

		let mut converter =
			XcmConverter::<MockTokenIdConvert, ()>::new(&message, network, Default::default());
		let result = converter.convert();
		assert_eq!(result.err(), Some(XcmConverterError::UnexpectedEndOfXcm));
	}

	#[test]
	fn xcm_converter_with_different_fee_asset_succeed() {
		let network = BridgedNetwork::get();

		let token_address: [u8; 20] = hex!("1000000000000000000000000000000000000000");
		let beneficiary_address: [u8; 20] = hex!("2000000000000000000000000000000000000000");

		let asset_location = [AccountKey20 { network: None, key: token_address }].into();
		let fee_asset =
			Asset { id: AssetId(Location { parents: 0, interior: Here }), fun: Fungible(1000) };

		let assets: Assets =
			vec![Asset { id: AssetId(asset_location), fun: Fungible(1000) }].into();

		let filter: AssetFilter = assets.clone().into();

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
			ClearOrigin,
			BuyExecution { fees: fee_asset, weight_limit: Unlimited },
			DepositAsset {
				assets: filter,
				beneficiary: AccountKey20 { network: None, key: beneficiary_address }.into(),
			},
			SetTopic([0; 32]),
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, ()>::new(&message, network, Default::default());
		let result = converter.convert();
		assert_eq!(result.is_ok(), true);
	}

	#[test]
	fn xcm_converter_with_fees_greater_than_reserve_succeed() {
		let network = BridgedNetwork::get();

		let token_address: [u8; 20] = hex!("1000000000000000000000000000000000000000");
		let beneficiary_address: [u8; 20] = hex!("2000000000000000000000000000000000000000");

		let asset_location: Location = [AccountKey20 { network: None, key: token_address }].into();
		let fee_asset = Asset { id: AssetId(asset_location.clone()), fun: Fungible(1001) };

		let assets: Assets =
			vec![Asset { id: AssetId(asset_location), fun: Fungible(1000) }].into();

		let filter: AssetFilter = assets.clone().into();

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
			ClearOrigin,
			BuyExecution { fees: fee_asset, weight_limit: Unlimited },
			DepositAsset {
				assets: filter,
				beneficiary: AccountKey20 { network: None, key: beneficiary_address }.into(),
			},
			SetTopic([0; 32]),
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, ()>::new(&message, network, Default::default());
		let result = converter.convert();
		assert_eq!(result.is_ok(), true);
	}

	#[test]
	fn xcm_converter_convert_with_empty_xcm_yields_unexpected_end_of_xcm() {
		let network = BridgedNetwork::get();

		let message: Xcm<()> = vec![].into();

		let mut converter =
			XcmConverter::<MockTokenIdConvert, ()>::new(&message, network, Default::default());

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

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
			ClearOrigin,
			BuyExecution { fees: assets.get(0).unwrap().clone(), weight_limit: Unlimited },
			DepositAsset {
				assets: filter,
				beneficiary: AccountKey20 { network: None, key: beneficiary_address }.into(),
			},
			SetTopic([0; 32]),
			ClearError,
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, ()>::new(&message, network, Default::default());

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
			XcmConverter::<MockTokenIdConvert, ()>::new(&message, network, Default::default());

		let result = converter.convert();
		assert_eq!(result.err(), Some(XcmConverterError::UnexpectedInstruction));
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

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
			ClearOrigin,
			BuyExecution { fees: assets.get(0).unwrap().clone(), weight_limit: Unlimited },
			SetTopic([0; 32]),
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, ()>::new(&message, network, Default::default());

		let result = converter.convert();
		assert_eq!(result.err(), Some(XcmConverterError::DepositAssetExpected));
	}

	#[test]
	fn xcm_converter_convert_without_assets_yields_no_reserve_assets() {
		let network = BridgedNetwork::get();

		let token_address: [u8; 20] = hex!("1000000000000000000000000000000000000000");

		let beneficiary_address: [u8; 20] = hex!("2000000000000000000000000000000000000000");

		let assets: Assets = vec![].into();
		let filter: AssetFilter = assets.clone().into();

		let fee = Asset {
			id: AssetId(AccountKey20 { network: None, key: token_address }.into()),
			fun: Fungible(1000),
		};

		let message: Xcm<()> = vec![
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
		let mut converter =
			XcmConverter::<MockTokenIdConvert, ()>::new(&message, network, Default::default());

		let result = converter.convert();
		assert_eq!(result.err(), Some(XcmConverterError::NoReserveAssets));
	}

	#[test]
	fn xcm_converter_convert_with_two_assets_yields_too_many_assets() {
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

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
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
			XcmConverter::<MockTokenIdConvert, ()>::new(&message, network, Default::default());

		let result = converter.convert();
		assert_eq!(result.err(), Some(XcmConverterError::TooManyAssets));
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

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
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
			XcmConverter::<MockTokenIdConvert, ()>::new(&message, network, Default::default());

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

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
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
			XcmConverter::<MockTokenIdConvert, ()>::new(&message, network, Default::default());

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

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
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
			XcmConverter::<MockTokenIdConvert, ()>::new(&message, network, Default::default());

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

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
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
			XcmConverter::<MockTokenIdConvert, ()>::new(&message, network, Default::default());

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

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
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
			XcmConverter::<MockTokenIdConvert, ()>::new(&message, network, Default::default());

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
		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
			ClearOrigin,
			BuyExecution { fees: assets.get(0).unwrap().clone(), weight_limit: Unlimited },
			DepositAsset {
				assets: filter,
				beneficiary: [
					GlobalConsensus(Polkadot),
					Parachain(1000),
					AccountId32 { network: Some(Polkadot), id: beneficiary_address },
				]
				.into(),
			},
			SetTopic([0; 32]),
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, ()>::new(&message, network, Default::default());

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

		let message: Xcm<()> = vec![
			WithdrawAsset(assets.clone()),
			ClearOrigin,
			BuyExecution { fees: assets.get(0).unwrap().clone(), weight_limit: Unlimited },
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
			XcmConverter::<MockTokenIdConvert, ()>::new(&message, network, Default::default());

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
		let asset_location = Location::new(1, [GlobalConsensus(Westend)]);
		let token_id = TokenIdOf::convert_location(&asset_location).unwrap();

		let assets: Assets =
			vec![Asset { id: AssetId(asset_location.clone()), fun: Fungible(amount) }].into();
		let filter: AssetFilter = assets.clone().into();

		let message: Xcm<()> = vec![
			ReserveAssetDeposited(assets.clone()),
			ClearOrigin,
			BuyExecution { fees: assets.get(0).unwrap().clone(), weight_limit: Unlimited },
			ExpectAsset(Asset { id: AssetId(asset_location), fun: Fungible(amount) }.into()),
			DepositAsset {
				assets: filter,
				beneficiary: AccountKey20 { network: None, key: beneficiary_address }.into(),
			},
			SetTopic([0; 32]),
		]
		.into();
		let mut converter =
			XcmConverter::<MockTokenIdConvert, ()>::new(&message, network, Default::default());
		let expected_payload =
			Command::MintForeignToken { recipient: beneficiary_address.into(), amount, token_id };
		let expected_message = Message {
			id: [0; 32].into(),
			origin: H256::zero(),
			fee: 1000000,
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
		let asset_location = Location { parents: 2, interior: [GlobalConsensus(Rococo)].into() };

		let assets: Assets =
			vec![Asset { id: AssetId(asset_location), fun: Fungible(amount) }].into();
		let filter: AssetFilter = assets.clone().into();

		let message: Xcm<()> = vec![
			ReserveAssetDeposited(assets.clone()),
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
			XcmConverter::<MockTokenIdConvert, ()>::new(&message, network, Default::default());
		let result = converter.convert();
		assert_eq!(result.err(), Some(XcmConverterError::InvalidAsset));
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

		let universal_source: InteriorLocation = [GlobalConsensus(Westend), Parachain(1000)].into();

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
