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
use crate::{Config, Pallet, Bridges, BridgeIdOf, LOG_TARGET};
use xcm_builder::ExporterFor;
use bp_xcm_bridge_hub_router::ResolveBridgeId;
use codec::Encode;
use frame_support::pallet_prelude::PhantomData;
use frame_support::traits::Get;
use frame_support::ensure;
use xcm::prelude::*;

/// Implementation of `LocalXcmChannelManager` which tracks and updates `is_congested` for a given `BridgeId`.
/// This implementation is useful for managing congestion and dynamic fees with the local `ExportXcm` implementation.
impl<T: Config<I>, I: 'static> bp_xcm_bridge_hub::LocalXcmChannelManager<BridgeIdOf<T, I>> for Pallet<T, I> {
    type Error = ();

    /// Suspends the given bridge.
    ///
    /// This function ensures that the `local_origin` matches the expected `Location::here()`. If the check passes, it updates the bridge status to congested.
    fn suspend_bridge(local_origin: &Location, bridge: BridgeIdOf<T, I>) -> Result<(), Self::Error> {
        log::trace!(
            target: LOG_TARGET,
            "LocalXcmChannelManager::suspend_bridge(local_origin: {local_origin:?}, bridge: {bridge:?})",
        );
        ensure!(local_origin.eq(&Location::here()), ());

        // update status
        Self::update_bridge_status(bridge, true);

        Ok(())
    }

    /// Resumes the given bridge.
    ///
    /// This function ensures that the `local_origin` matches the expected `Location::here()`. If the check passes, it updates the bridge status to not congested.
    fn resume_bridge(local_origin: &Location, bridge: BridgeIdOf<T, I>) -> Result<(), Self::Error> {
        log::trace!(
            target: LOG_TARGET,
            "LocalXcmChannelManager::resume_bridge(local_origin: {local_origin:?}, bridge: {bridge:?})",
        );
        ensure!(local_origin.eq(&Location::here()), ());

        // update status
        Self::update_bridge_status(bridge, false);

        Ok(())
    }
}

pub struct ViaRemoteBridgeHubExporter<T, I, E, BNF, BHLF>(PhantomData<(T, I, E, BNF, BHLF)>);

impl<T: Config<I>, I: 'static, E, BridgedNetworkIdFilter, BridgeHubLocationFilter> ExporterFor for ViaRemoteBridgeHubExporter<T, I, E, BridgedNetworkIdFilter, BridgeHubLocationFilter>
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
        let (bridge_hub_location, maybe_payment) = match E::exporter_for(network, remote_location, message) {
            Some((bridge_hub_location, maybe_payment)) => match BridgeHubLocationFilter::get() {
                Some(expected_bridge_hub_location) if expected_bridge_hub_location.eq(&bridge_hub_location) => (bridge_hub_location, maybe_payment),
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
                }
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
        let maybe_message_size_fees = Pallet::<T, I>::calculate_message_size_fees(|| message.encoded_size() as _);

        // compute actual fees - sum(actual payment, message size fees) if possible
        let fees = match (maybe_payment, maybe_message_size_fees)  {
            (Some(payment), None) => Some(payment),
            (None, Some(message_size_fees)) => Some(message_size_fees),
            (None, None) => None,
            (
                Some(Asset {id: payment_asset_id, fun: Fungible(payment_amount)}),
                Some(Asset {id: message_size_fees_asset_id, fun: Fungible(message_size_fees_amount)})
            ) if payment_asset_id.eq(&message_size_fees_asset_id) => {
                // we can subsume two assets with the same asset_id and fungibility.
                Some((payment_asset_id, (payment_amount.saturating_add(message_size_fees_amount))).into())
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
            }
        };

        // Here, we have the actual result fees covering bridge fees, so now we need to check/apply the congestion and dynamic_fees features (if possible).
        let fees = fees.map(|fees| if let Some(bridge_id) = T::BridgeIdResolver::resolve_for(network, remote_location) {
            if let Some(bridge_state) = Bridges::<T, I>::get(bridge_id) {
                Pallet::<T, I>::calculate_dynamic_fees_for_asset(&bridge_state, fees)
            } else {
                fees
            }
        } else {
            fees
        });

        Some((bridge_hub_location, fees))
    }
}
