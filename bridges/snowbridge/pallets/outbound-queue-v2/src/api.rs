// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Helpers for implementing runtime api

use crate::{Config, MessageLeaves};
use frame_support::storage::StorageStreamIter;
use snowbridge_core::{
	outbound::{
		v2::{
			abi::{CommandWrapper, InboundMessage},
			GasMeter, Message,
		},
		DryRunError,
	},
	AgentIdOf,
};
use snowbridge_merkle_tree::{merkle_proof, MerkleProof};
use snowbridge_router_primitives::outbound::v2::convert::XcmConverter;
use sp_core::Get;
use sp_std::{default::Default, vec::Vec};
use xcm::{
	latest::Location,
	prelude::{Parachain, Xcm},
};
use xcm_executor::traits::ConvertLocation;

pub fn prove_message<T>(leaf_index: u64) -> Option<MerkleProof>
where
	T: Config,
{
	if !MessageLeaves::<T>::exists() {
		return None
	}
	let proof =
		merkle_proof::<<T as Config>::Hashing, _>(MessageLeaves::<T>::stream_iter(), leaf_index);
	Some(proof)
}

pub fn dry_run<T>(xcm: Xcm<()>) -> Result<(InboundMessage, T::Balance), DryRunError>
where
	T: Config,
{
	let mut converter = XcmConverter::<T::ConvertAssetId, ()>::new(
		&xcm,
		T::EthereumNetwork::get(),
		AgentIdOf::convert_location(&Location::new(1, Parachain(1000)))
			.ok_or(DryRunError::ConvertLocationFailed)?,
	);

	let message: Message = converter.convert().map_err(|_| DryRunError::ConvertXcmFailed)?;

	let fee = crate::Pallet::<T>::calculate_local_fee();

	let commands: Vec<CommandWrapper> = message
		.commands
		.into_iter()
		.map(|command| CommandWrapper {
			kind: command.index(),
			gas: T::GasMeter::maximum_dispatch_gas_used_at_most(&command),
			payload: command.abi_encode(),
		})
		.collect();

	let committed_message = InboundMessage {
		origin: message.origin,
		nonce: Default::default(),
		commands: commands.try_into().map_err(|_| DryRunError::ConvertXcmFailed)?,
	};

	Ok((committed_message, fee))
}
