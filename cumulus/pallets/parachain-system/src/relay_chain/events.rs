// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Something that should be informed about system related events.

use cumulus_primitives_core::PersistedValidationData;
use frame_support::weights::Weight;

use crate::relay_chain::relay_state_snapshot::RelayChainStateProof;

/// Something that should be informed about system related events.
///
/// This includes events like [`on_validation_data`](Self::on_validation_data) that is being
/// called when the parachain inherent is executed that contains the validation data.
/// Or like [`on_validation_code_applied`](Self::on_validation_code_applied) that is called
/// when the new validation is written to the state. This means that
/// from the next block the runtime is being using this new code.
pub trait OnSystemEvent {
	/// Called in each blocks once when the validation data is set by the inherent.
	fn on_validation_data(data: &PersistedValidationData);
	/// Called when the validation code is being applied, aka from the next block on this is the new
	/// runtime.
	fn on_validation_code_applied();
	/// Called to process keys from the verified relay chain state proof.
	fn on_relay_state_proof(relay_state_proof: &RelayChainStateProof) -> Weight;
}

#[impl_trait_for_tuples::impl_for_tuples(30)]
impl OnSystemEvent for Tuple {
	fn on_validation_data(data: &PersistedValidationData) {
		for_tuples!( #( Tuple::on_validation_data(data); )* );
	}

	fn on_validation_code_applied() {
		for_tuples!( #( Tuple::on_validation_code_applied(); )* );
	}

	fn on_relay_state_proof(relay_state_proof: &RelayChainStateProof) -> Weight {
		let mut weight = Weight::zero();
		for_tuples!( #( weight = weight.saturating_add(Tuple::on_relay_state_proof(relay_state_proof)); )* );
		weight
	}
}
