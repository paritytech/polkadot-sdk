// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use codec::{Decode, Encode};
use core::marker::PhantomData;
use frame_support::{
	traits::{Contains, Get},
	weights::Weight,
};
use snowbridge_core::{operating_mode::ExportPausedQuery, ParaId, TokenId};
use sp_runtime::traits::MaybeConvert;
use sp_std::{prelude::*, vec::Vec};
use xcm::{
	latest::{ExecuteXcm, InstructionError, Outcome, PreparedMessage},
	prelude::{
		Asset, Assets, Here, Instruction, InteriorLocation, Junctions, Location, NetworkId,
		Parachain, SendError, SendResult, SendXcm, Xcm, XcmHash,
	},
	VersionedLocation, VersionedXcm,
};
use xcm_builder::{ExporterFor, InspectMessageQueues};
use xcm_executor::traits::ExportXcm;

use crate::v2::{
	converter::{
		convert::{ensure_top_level_optional_contract_call_params_valid, XcmConverterError},
		XcmConverter,
	},
	message::SendMessage,
};

const TARGET: &str = "xcm::ethereum_blob_exporter::v2";

const EXECUTION_TARGET: &str = "xcm::ethereum_execution_exporter";

/// Same routing gates as the beginning of [`EthereumBlobExporter::validate`] (Ethereum network,
/// `Here` destination, universal source = relay global consensus + Asset Hub parachain).
///
/// On success returns the `local_sub` [`Junctions`] after [`InteriorLocation::split_global`], for
/// example to build an XCM origin `Location::new(1, local_sub)`.
pub fn validate_ethereum_blob_exporter_v2_route<
	UniversalLocation,
	EthereumNetwork,
	AssetHubParaId,
>(
	network: NetworkId,
	universal_source: &Option<InteriorLocation>,
	destination: &Option<InteriorLocation>,
) -> Result<Junctions, SendError>
where
	UniversalLocation: Get<InteriorLocation>,
	EthereumNetwork: Get<NetworkId>,
	AssetHubParaId: Get<ParaId>,
{
	let expected_network = EthereumNetwork::get();
	let universal_location = UniversalLocation::get();

	if network != expected_network {
		tracing::trace!(target: TARGET, ?network, "skipped due to unmatched bridge network.");
		return Err(SendError::NotApplicable);
	}

	let dest = destination.clone().ok_or(SendError::MissingArgument)?;
	if dest != Here {
		tracing::trace!(target: TARGET, destination=?dest, "skipped due to unmatched remote destination.");
		return Err(SendError::NotApplicable);
	}

	let (local_net, local_sub) = universal_source
		.clone()
		.ok_or_else(|| {
			tracing::error!(target: TARGET, "universal source not provided.");
			SendError::MissingArgument
		})?
		.split_global()
		.map_err(|()| {
			tracing::error!(target: TARGET, ?universal_source, "could not get global consensus.");
			SendError::NotApplicable
		})?;

	if Ok(local_net) != universal_location.global_consensus() {
		tracing::trace!(target: TARGET, relay_network=?local_net, "skipped due to unmatched relay network.");
		return Err(SendError::NotApplicable);
	}

	let para_id = match local_sub.as_slice() {
		[Parachain(para_id)] => *para_id,
		_ => {
			tracing::error!(target: TARGET, universal_source=?local_sub, "could not get parachain id.");
			return Err(SendError::NotApplicable);
		},
	};

	if ParaId::from(para_id) != AssetHubParaId::get() {
		tracing::error!(target: TARGET, ?para_id, "is not from asset hub.");
		return Err(SendError::NotApplicable);
	}

	Ok(local_sub)
}

/// Returns `true` if the top-level instruction list contains an [`Instruction::AliasOrigin`]
/// instruction.
///
/// Snowbridge v2 outbound export blobs include `AliasOrigin`; legacy v1 blobs do not. Used for the
/// v1/v2 routing predicate (same as [`EthereumBlobExporter`] and Bridge Hub export simulation).
pub fn snowbridge_v2_instructions_contain_alias_origin<Call>(
	instructions: &[Instruction<Call>],
) -> bool {
	instructions.iter().any(|i| matches!(i, Instruction::AliasOrigin(_)))
}

/// Like [`snowbridge_v2_instructions_contain_alias_origin`] for an [`Xcm`] program.
///
/// [`ExecuteBeforeSnowbridgeV2BlobExport`] uses this before running export simulation so the inner
/// [`EthereumBlobExporter`] only sees v2-shaped messages (the inner exporter no longer repeats this
/// check).
pub fn snowbridge_v2_export_blob_contains_alias_origin(xcm: &Xcm<()>) -> bool {
	snowbridge_v2_instructions_contain_alias_origin(xcm.inner())
}

/// Used to process ExportMessages where the destination is Ethereum. It takes an ExportMessage
/// and converts it into a simpler message that the Ethereum gateway contract can understand.
pub struct EthereumBlobExporter<
	UniversalLocation,
	EthereumNetwork,
	OutboundQueue,
	ConvertAssetId,
	AssetHubParaId,
>(
	PhantomData<(
		UniversalLocation,
		EthereumNetwork,
		OutboundQueue,
		ConvertAssetId,
		AssetHubParaId,
	)>,
);

impl<UniversalLocation, EthereumNetwork, OutboundQueue, ConvertAssetId, AssetHubParaId> ExportXcm
	for EthereumBlobExporter<
		UniversalLocation,
		EthereumNetwork,
		OutboundQueue,
		ConvertAssetId,
		AssetHubParaId,
	>
where
	UniversalLocation: Get<InteriorLocation>,
	EthereumNetwork: Get<NetworkId>,
	OutboundQueue: SendMessage,
	ConvertAssetId: MaybeConvert<TokenId, Location>,
	AssetHubParaId: Get<ParaId>,
{
	type Ticket = (Vec<u8>, XcmHash);

	fn validate(
		network: NetworkId,
		_channel: u32,
		universal_source: &mut Option<InteriorLocation>,
		destination: &mut Option<InteriorLocation>,
		message: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		tracing::debug!(target: TARGET, ?message, "message route through bridge.");

		validate_ethereum_blob_exporter_v2_route::<
			UniversalLocation,
			EthereumNetwork,
			AssetHubParaId,
		>(network, universal_source, destination)?;

		let message = message.clone().ok_or_else(|| {
			tracing::error!(target: TARGET, "xcm message not provided.");
			SendError::MissingArgument
		})?;

		let mut converter = XcmConverter::<ConvertAssetId, ()>::new(&message, network);
		let message = converter.convert().map_err(|err| {
			tracing::error!(target: TARGET, error=?err, "unroutable due to pattern matching.");
			SendError::Unroutable
		})?;

		// validate the message
		let ticket = OutboundQueue::validate(&message).map_err(|err| {
			tracing::error!(target: TARGET, error=?err, "OutboundQueue validation of message failed.");
			SendError::Unroutable
		})?;

		Ok(((ticket.encode(), XcmHash::from(message.id)), Assets::default()))
	}

	fn deliver(blob: (Vec<u8>, XcmHash)) -> Result<XcmHash, SendError> {
		let ticket: OutboundQueue::Ticket = OutboundQueue::Ticket::decode(&mut blob.0.as_ref())
			.map_err(|_| {
				tracing::trace!(target: TARGET, "undeliverable due to decoding error");
				SendError::NotApplicable
			})?;

		let message_id = OutboundQueue::deliver(ticket).map_err(|_| {
			tracing::error!(target: TARGET, "OutboundQueue submit of message failed");
			SendError::Transport("other transport error")
		})?;

		tracing::info!(target: TARGET, "message delivered {message_id:#?}.");
		Ok(message_id.into())
	}
}

/// An adapter for the implementation of `ExporterFor`, which attempts to find the
/// `(bridge_location, payment)` for the requested `network` and `remote_location` and `xcm`
/// in the provided `T` table containing various exporters.
pub struct XcmFilterExporter<T, M>(PhantomData<(T, M)>);
impl<T: ExporterFor, M: Contains<Xcm<()>>> ExporterFor for XcmFilterExporter<T, M> {
	fn exporter_for(
		network: &NetworkId,
		remote_location: &InteriorLocation,
		xcm: &Xcm<()>,
	) -> Option<(Location, Option<Asset>)> {
		// check the XCM
		if !M::contains(xcm) {
			return None;
		}
		// check `network` and `remote_location`
		T::exporter_for(network, remote_location, xcm)
	}
}

/// Xcm for SnowbridgeV2 which requires XCMV5
pub struct XcmForSnowbridgeV2;
impl Contains<Xcm<()>> for XcmForSnowbridgeV2 {
	fn contains(xcm: &Xcm<()>) -> bool {
		snowbridge_v2_export_blob_contains_alias_origin(xcm)
	}
}

pub struct PausableExporter<PausedQuery, InnerExporter>(PhantomData<(PausedQuery, InnerExporter)>);

impl<PausedQuery: ExportPausedQuery, InnerExporter: SendXcm> SendXcm
	for PausableExporter<PausedQuery, InnerExporter>
{
	type Ticket = InnerExporter::Ticket;

	fn validate(
		destination: &mut Option<Location>,
		message: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		match PausedQuery::is_paused() {
			true => Err(SendError::NotApplicable),
			false => InnerExporter::validate(destination, message),
		}
	}

	fn deliver(ticket: Self::Ticket) -> Result<XcmHash, SendError> {
		match PausedQuery::is_paused() {
			true => Err(SendError::NotApplicable),
			false => InnerExporter::deliver(ticket),
		}
	}
}

impl<Halted: ExportPausedQuery, InnerExporter: SendXcm> InspectMessageQueues
	for PausableExporter<Halted, InnerExporter>
{
	fn clear_messages() {}

	/// This router needs to implement `InspectMessageQueues` but doesn't have to
	/// return any messages, since it just reuses the inner router.
	fn get_messages() -> Vec<(VersionedLocation, Vec<VersionedXcm<()>>)> {
		Vec::new()
	}
}

/// Replace Ethereum-bound `Transact` payloads (not decodable as local `RuntimeCall`) with
/// no-op stand-ins for dry-run only; real export still uses the original message in
/// [`ExportXcm::validate`] on the inner exporter.
pub fn neutralize_eth_export_transacts_in_xcm_runtime<Call>(xcm: &mut Xcm<Call>) {
	for instruction in xcm.0.iter_mut() {
		match instruction {
			Instruction::Transact { .. } => *instruction = Instruction::ClearError,
			_ => {},
		}
	}
}

/// Forces the Bridge Hub dry-run clone to trap holding.
///
/// Used only on the simulation clone when the original message contains an invalid or malformed
/// [`ContractCall::V1`]. This keeps the real export payload unchanged while forcing simulated
/// execution to halt with assets still in holding so the normal BH `AssetTrap` path is exercised.
fn force_trap_holding_in_simulation_xcm_runtime<Call>(xcm: &mut Xcm<Call>) -> bool {
	for instruction in xcm.0.iter_mut() {
		match instruction {
			Instruction::DepositAsset { .. } => {
				// Force trapping of the holding register in simulation
				*instruction = Instruction::Trap(0);
				return true;
			},
			_ => {},
		}
	}

	false
}

/// Runs the outbound XCM under `Executor` before delegating to the inner Snowbridge v2 blob
/// exporter, so Ethereum-semantic instructions are exercised in simulation.
///
/// If the inner export blob has no [`Instruction::AliasOrigin`], returns
/// [`SendError::NotApplicable`] so a composed [`ExportXcm`] tuple can try the legacy v1 exporter
/// (see [`snowbridge_v2_export_blob_contains_alias_origin`]).
pub struct ExecuteBeforeSnowbridgeV2BlobExport<
	Inner,
	Executor,
	UniversalLocation,
	EthereumNetworkTy,
	AssetHubParaIdTy,
	Call,
>(PhantomData<(Inner, Executor, UniversalLocation, EthereumNetworkTy, AssetHubParaIdTy, Call)>);

impl<
		Inner: ExportXcm,
		Executor: ExecuteXcm<Call>,
		UniversalLocation: Get<InteriorLocation>,
		EthereumNetworkTy: Get<NetworkId>,
		AssetHubParaIdTy: Get<ParaId>,
		Call,
	> ExportXcm
	for ExecuteBeforeSnowbridgeV2BlobExport<
		Inner,
		Executor,
		UniversalLocation,
		EthereumNetworkTy,
		AssetHubParaIdTy,
		Call,
	>
{
	type Ticket = Inner::Ticket;

	fn validate(
		network: NetworkId,
		channel: u32,
		universal_source: &mut Option<InteriorLocation>,
		destination: &mut Option<InteriorLocation>,
		message: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		let local_sub = validate_ethereum_blob_exporter_v2_route::<
			UniversalLocation,
			EthereumNetworkTy,
			AssetHubParaIdTy,
		>(network, universal_source, destination)
		.map_err(|_| {
			tracing::error!(target: TARGET, "Failed to validate ethereum execution exporter route");
			SendError::NotApplicable
		})?;

		let msg_ref = message.as_ref().ok_or(SendError::MissingArgument)?;
		if !snowbridge_v2_export_blob_contains_alias_origin(msg_ref) {
			return Err(SendError::NotApplicable);
		}

		let msg = msg_ref.clone();
		let mut msg: Xcm<Call> = msg.into();
		if matches!(
			ensure_top_level_optional_contract_call_params_valid(msg_ref.inner()),
			Err(XcmConverterError::InvalidContractCallParams |
				XcmConverterError::TransactDecodeFailed)
		) && !force_trap_holding_in_simulation_xcm_runtime(&mut msg)
		{
			tracing::warn!(
				target: EXECUTION_TARGET,
				"invalid or malformed ContractCall detected but no DepositAsset found to force trap",
			);
		}
		neutralize_eth_export_transacts_in_xcm_runtime(&mut msg);
		let origin = Location::new(1, local_sub);
		let prepared =
			Executor::prepare(msg.clone(), Weight::MAX).map_err(|e: InstructionError| {
				tracing::error!(
					target: EXECUTION_TARGET,
					?e,
					"Failed to prepare ethereum XCM message: {:?}",
					msg,
				);
				SendError::Unroutable
			})?;
		let exec_weight = prepared.weight_of();
		match Executor::execute(origin, prepared, &mut XcmHash::default(), exec_weight) {
			Outcome::Complete { .. } => {},
			Outcome::Incomplete { error, .. } | Outcome::Error(error) => {
				tracing::error!(
					target: EXECUTION_TARGET,
					?error,
					"Failed to execute ethereum XCM message: {:?}",
					message.clone(),
				);
				return Err(SendError::Unroutable);
			},
		}

		Inner::validate(network, channel, universal_source, destination, message)
	}

	fn deliver(ticket: Self::Ticket) -> Result<XcmHash, SendError> {
		Inner::deliver(ticket)
	}
}
