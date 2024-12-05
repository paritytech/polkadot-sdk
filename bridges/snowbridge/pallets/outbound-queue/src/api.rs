// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Helpers for implementing runtime api

use crate::{Config, MessageLeaves};
use frame_support::storage::StorageStreamIter;
use snowbridge_core::{
	outbound::{Command, Fee, GasMeter},
	PricingParameters,
};
use snowbridge_outbound_queue_merkle_tree::{merkle_proof, MerkleProof};
use sp_core::Get;

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

pub fn calculate_fee<T>(
	command: Command,
	parameters: Option<PricingParameters<T::Balance>>,
) -> Fee<T::Balance>
where
	T: Config,
{
	let gas_used_at_most = T::GasMeter::maximum_gas_used_at_most(&command);
	let parameters = parameters.unwrap_or(T::PricingParameters::get());
	crate::Pallet::<T>::calculate_fee(gas_used_at_most, parameters)
}
