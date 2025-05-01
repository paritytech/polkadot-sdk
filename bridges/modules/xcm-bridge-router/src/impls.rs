// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Various implementations supporting easier configuration of the pallet.

use crate::{BridgeIdOf, Bridges, Config, Pallet, LOG_TARGET};
use bp_xcm_bridge_router::ResolveBridgeId;
use codec::Encode;
use frame_support::{ensure, pallet_prelude::PhantomData, traits::Get};
use xcm::prelude::*;
use xcm_builder::{ensure_is_remote, ExporterFor};

/// Implementation of [`bp_xcm_bridge::LocalXcmChannelManager`] which tracks and updates
/// `is_congested` for a given `BridgeId`. This implementation is useful for managing congestion and
/// dynamic fees with the local `ExportXcm` implementation.
impl<T: Config<I>, I: 'static> bp_xcm_bridge::LocalXcmChannelManager<BridgeIdOf<T, I>>
	for Pallet<T, I>
{
	type Error = ();

	/// Suspends the given bridge.
	///
	/// This function ensures that the `local_origin` matches the expected `Location::here()`. If
	/// the check passes, it updates the bridge status to congested.
	fn suspend_bridge(
		local_origin: &Location,
		bridge: BridgeIdOf<T, I>,
	) -> Result<(), Self::Error> {
		log::trace!(
			target: LOG_TARGET,
			"LocalXcmChannelManager::suspend_bridge(local_origin: {local_origin:?}, bridge: {bridge:?})",
		);
		ensure!(local_origin.eq(&Location::here()), ());

		// update status
		Self::do_update_bridge_status(bridge, true);

		Ok(())
	}

	/// Resumes the given bridge.
	///
	/// This function ensures that the `local_origin` matches the expected `Location::here()`. If
	/// the check passes, it updates the bridge status to not congested.
	fn resume_bridge(local_origin: &Location, bridge: BridgeIdOf<T, I>) -> Result<(), Self::Error> {
		log::trace!(
			target: LOG_TARGET,
			"LocalXcmChannelManager::resume_bridge(local_origin: {local_origin:?}, bridge: {bridge:?})",
		);
		ensure!(local_origin.eq(&Location::here()), ());

		// update status
		Self::do_update_bridge_status(bridge, false);

		Ok(())
	}
}

/// Adapter implementation for [`ExporterFor`] that allows exporting message size fee and/or dynamic
/// fees based on the `BridgeId` resolved by the `T::BridgeIdResolver` resolver, if and only if the
/// `E` exporter supports bridging. This adapter acts as an [`ExporterFor`], for example, for the
/// [`xcm_builder::SovereignPaidRemoteExporter`], enabling it to compute message and/or dynamic fees
/// using a fee factor.
pub struct ViaRemoteBridgeExporter<T, I, E, BNF, BHLF>(PhantomData<(T, I, E, BNF, BHLF)>);
impl<T: Config<I>, I: 'static, E, BridgedNetworkIdFilter, BridgeHubLocationFilter> ExporterFor
	for ViaRemoteBridgeExporter<T, I, E, BridgedNetworkIdFilter, BridgeHubLocationFilter>
where
	E: ExporterFor,
	BridgedNetworkIdFilter: Get<Option<NetworkId>>,
	BridgeHubLocationFilter: Get<Option<Location>>,
{
	fn exporter_for(
		network: &NetworkId,
		remote_location: &InteriorLocation,
		message: &Xcm<()>,
	) -> Option<(Location, Option<Asset>)> {
		log::trace!(
			target: LOG_TARGET,
			"exporter_for - network: {network:?}, remote_location: {remote_location:?}, msg: {message:?}",
		);
		// ensure that the message is sent to the expected bridged network (if specified).
		if let Some(bridged_network) = BridgedNetworkIdFilter::get() {
			if *network != bridged_network {
				log::trace!(
					target: LOG_TARGET,
					"Router with bridged_network_id filter({bridged_network:?}) does not support bridging to network {network:?}!",
				);
				return None
			}
		}

		// ensure that the message is sent to the expected bridged network and location.
		let (bridge_hub_location, maybe_payment) = match E::exporter_for(
			network,
			remote_location,
			message,
		) {
			Some((bridge_hub_location, maybe_payment)) => match BridgeHubLocationFilter::get() {
				Some(expected_bridge_hub_location)
					if expected_bridge_hub_location.eq(&bridge_hub_location) =>
					(bridge_hub_location, maybe_payment),
				None => (bridge_hub_location, maybe_payment),
				_ => {
					log::trace!(
						target: LOG_TARGET,
						"Resolved bridge_hub_location: {:?} does not match expected one: {:?} for bridging to network {:?} and remote_location {:?}!",
						bridge_hub_location,
						BridgeHubLocationFilter::get(),
						network,
						remote_location,
					);
					return None
				},
			},
			_ => {
				log::trace!(
					target: LOG_TARGET,
					"Inner `E` router does not support bridging to network {:?} and remote_location {:?}!",
					network,
					remote_location,
				);
				return None
			},
		};

		// calculate message size fees (if configured)
		let maybe_message_size_fees =
			Pallet::<T, I>::calculate_message_size_fee(|| message.encoded_size() as _);

		// compute actual fees - sum(actual payment, message size fees) if possible
		let mut fees = match (maybe_payment, maybe_message_size_fees) {
			(Some(payment), None) => Some(payment),
			(None, Some(message_size_fees)) => Some(message_size_fees),
			(None, None) => None,
			(
				Some(Asset { id: payment_asset_id, fun: Fungible(payment_amount) }),
				Some(Asset {
					id: message_size_fees_asset_id,
					fun: Fungible(message_size_fees_amount),
				}),
			) if payment_asset_id.eq(&message_size_fees_asset_id) => {
				// we can subsume two assets with the same asset_id and fungibility.
				Some(
					(payment_asset_id, payment_amount.saturating_add(message_size_fees_amount))
						.into(),
				)
			},
			(Some(payment), Some(message_size_fees)) => {
				log::error!(
					target: LOG_TARGET,
					"Router is configured for `T::FeeAsset` {:?} \
					but we have two different assets which cannot be calculated as one result asset: payment: {:?} and message_size_fees: {:?} for bridge_hub_location: {:?} for bridging to {:?}/{:?}!",
					T::FeeAsset::get(),
					payment,
					message_size_fees,
					bridge_hub_location,
					network,
					remote_location,
				);
				return None
			},
		};

		// `fees` is populated with base bridge fees, now let's apply congestion/dynamic fees if
		// required.
		if let Some(bridge_id) = T::BridgeIdResolver::resolve_for(network, remote_location) {
			if let Some(bridge_state) = Bridges::<T, I>::get(bridge_id) {
				if let Some(f) = fees {
					fees = Some(Pallet::<T, I>::apply_dynamic_fee_factor(&bridge_state, f));
				}
			}
		}

		Some((bridge_hub_location, fees))
	}
}

/// Adapter implementation for [`SendXcm`] that allows adding a message size fee and/or dynamic fees
/// based on the `BridgeId` resolved by the `T::BridgeIdResolver` resolver, if and only if `E`
/// supports routing. This adapter can be used, for example, as a wrapper over
/// [`xcm_builder::LocalExporter`], enabling it to compute message and/or dynamic fees using a
/// fee factor.
pub struct ViaLocalBridgeExporter<T, I, E>(PhantomData<(T, I, E)>);
impl<T: Config<I>, I: 'static, E: SendXcm> SendXcm for ViaLocalBridgeExporter<T, I, E> {
	type Ticket = E::Ticket;

	fn validate(
		destination: &mut Option<Location>,
		message: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		let dest_clone = destination.clone().ok_or(SendError::MissingArgument)?;
		let message_size = message.as_ref().map_or(0, |message| message.encoded_size()) as _;

		match E::validate(destination, message) {
			Ok((ticket, mut fees)) => {
				// calculate message size fees (if configured)
				let maybe_message_size_fees =
					Pallet::<T, I>::calculate_message_size_fee(|| message_size);
				if let Some(message_size_fees) = maybe_message_size_fees {
					fees.push(message_size_fees);
				}

				// Here, we have the actual result fees covering bridge fees, so now we need to
				// check/apply the congestion and dynamic_fees features (if possible).
				if let Some(bridge_id) = T::BridgeIdResolver::resolve_for_dest(&dest_clone) {
					if let Some(bridge_state) = Bridges::<T, I>::get(bridge_id) {
						let mut dynamic_fees = sp_std::vec::Vec::with_capacity(fees.len());
						for fee in fees.into_inner() {
							dynamic_fees
								.push(Pallet::<T, I>::apply_dynamic_fee_factor(&bridge_state, fee));
						}
						fees = Assets::from(dynamic_fees);
					}
				}

				// return original ticket with possibly extended fees
				Ok((ticket, fees))
			},
			error => error,
		}
	}

	fn deliver(ticket: Self::Ticket) -> Result<XcmHash, SendError> {
		E::deliver(ticket)
	}
}

/// Implementation of [`ResolveBridgeId`] returning [`bp_xcm_bridge::BridgeId`] based on the
/// configured `UniversalLocation` and remote universal location.
pub struct EnsureIsRemoteBridgeIdResolver<UniversalLocation>(PhantomData<UniversalLocation>);
impl<UniversalLocation: Get<InteriorLocation>> ResolveBridgeId
	for EnsureIsRemoteBridgeIdResolver<UniversalLocation>
{
	type BridgeId = bp_xcm_bridge::BridgeId;

	fn resolve_for_dest(dest: &Location) -> Option<Self::BridgeId> {
		let Ok((remote_network, remote_dest)) =
			ensure_is_remote(UniversalLocation::get(), dest.clone())
		else {
			log::trace!(
				target: LOG_TARGET,
				"EnsureIsRemoteBridgeIdResolver - does not recognize a remote destination for: {dest:?}!"
			);
			return None
		};
		Self::resolve_for(&remote_network, &remote_dest)
	}

	fn resolve_for(
		bridged_network: &NetworkId,
		bridged_dest: &InteriorLocation,
	) -> Option<Self::BridgeId> {
		let bridged_universal_location = if let Ok(network) = bridged_dest.global_consensus() {
			if network.ne(bridged_network) {
				log::error!(
					target: LOG_TARGET,
					"EnsureIsRemoteBridgeIdResolver - bridged_dest: {bridged_dest:?} contains invalid network: {network:?}, expected bridged_network: {bridged_network:?}!"
				);
				return None
			} else {
				bridged_dest.clone()
			}
		} else {
			// if `bridged_dest` does not contain `GlobalConsensus`, let's prepend one
			match bridged_dest.clone().pushed_front_with(*bridged_network) {
				Ok(bridged_universal_location) => bridged_universal_location,
				Err((original, prepend_with)) => {
					log::error!(
						target: LOG_TARGET,
						"EnsureIsRemoteBridgeIdResolver - bridged_dest: {original:?} cannot be prepended with: {prepend_with:?}!"
					);
					return None
				},
			}
		};

		match (
			UniversalLocation::get().global_consensus(),
			bridged_universal_location.global_consensus(),
		) {
			(Ok(local), Ok(remote)) if local != remote => (),
			(local, remote) => {
				log::error!(
					target: LOG_TARGET,
					"EnsureIsRemoteBridgeIdResolver - local: {local:?} and remote: {remote:?} must be different!"
				);
				return None
			},
		}

		// calculate `BridgeId` from universal locations
		Some(Self::BridgeId::new(&UniversalLocation::get(), &bridged_universal_location))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn ensure_is_remote_bridge_id_resolver_works() {
		frame_support::parameter_types! {
			pub ThisNetwork: NetworkId = NetworkId::ByGenesis([0; 32]);
			pub BridgedNetwork: NetworkId = NetworkId::ByGenesis([1; 32]);
			pub UniversalLocation: InteriorLocation = [GlobalConsensus(ThisNetwork::get()), Parachain(1000)].into();
		}
		assert_ne!(ThisNetwork::get(), BridgedNetwork::get());

		type Resolver = EnsureIsRemoteBridgeIdResolver<UniversalLocation>;

		// not remote dest
		assert!(Resolver::resolve_for_dest(&Location::new(1, Here)).is_none());
		// not a valid remote dest
		assert!(Resolver::resolve_for_dest(&Location::new(2, Here)).is_none());
		// the same network for remote dest
		assert!(Resolver::resolve_for_dest(&Location::new(2, GlobalConsensus(ThisNetwork::get())))
			.is_none());
		assert!(Resolver::resolve_for(&ThisNetwork::get(), &Here.into()).is_none());

		// ok
		assert!(Resolver::resolve_for_dest(&Location::new(
			2,
			GlobalConsensus(BridgedNetwork::get())
		))
		.is_some());
		assert!(Resolver::resolve_for_dest(&Location::new(
			2,
			[GlobalConsensus(BridgedNetwork::get()), Parachain(2013)]
		))
		.is_some());

		// ok - resolves the same
		assert_eq!(
			Resolver::resolve_for_dest(&Location::new(2, GlobalConsensus(BridgedNetwork::get()))),
			Resolver::resolve_for(&BridgedNetwork::get(), &Here.into()),
		);
		assert_eq!(
			Resolver::resolve_for_dest(&Location::new(
				2,
				[GlobalConsensus(BridgedNetwork::get()), Parachain(2013)]
			)),
			Resolver::resolve_for(&BridgedNetwork::get(), &Parachain(2013).into()),
		);
		assert_eq!(
			Resolver::resolve_for_dest(&Location::new(
				2,
				[GlobalConsensus(BridgedNetwork::get()), Parachain(2013)]
			)),
			Resolver::resolve_for(
				&BridgedNetwork::get(),
				&[GlobalConsensus(BridgedNetwork::get()), Parachain(2013)].into()
			),
		);
	}
}
