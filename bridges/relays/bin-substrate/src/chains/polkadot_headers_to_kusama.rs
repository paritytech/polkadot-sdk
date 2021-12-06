// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

//! Polkadot-to-Kusama headers sync entrypoint.

use sp_core::Pair;
use substrate_relay_helper::{finality_pipeline::SubstrateFinalitySyncPipeline, TransactionParams};

/// Maximal saturating difference between `balance(now)` and `balance(now-24h)` to treat
/// relay as gone wild.
///
/// Actual value, returned by `maximal_balance_decrease_per_day_is_sane` test is approximately 0.001
/// KSM, but let's round up to 0.1 KSM here.
pub(crate) const MAXIMAL_BALANCE_DECREASE_PER_DAY: bp_polkadot::Balance = 100_000_000_000;

/// Description of Polkadot -> Kusama finalized headers bridge.
#[derive(Clone, Debug)]
pub struct PolkadotFinalityToKusama;
substrate_relay_helper::generate_mocked_submit_finality_proof_call_builder!(
	PolkadotFinalityToKusama,
	PolkadotFinalityToKusamaCallBuilder,
	relay_kusama_client::runtime::Call::BridgePolkadotGrandpa,
	relay_kusama_client::runtime::BridgePolkadotGrandpaCall::submit_finality_proof
);

impl SubstrateFinalitySyncPipeline for PolkadotFinalityToKusama {
	type SourceChain = relay_polkadot_client::Polkadot;
	type TargetChain = relay_kusama_client::Kusama;

	type SubmitFinalityProofCallBuilder = PolkadotFinalityToKusamaCallBuilder;
	type TransactionSignScheme = relay_kusama_client::Kusama;

	fn start_relay_guards(
		target_client: &relay_substrate_client::Client<relay_kusama_client::Kusama>,
		transaction_params: &TransactionParams<sp_core::sr25519::Pair>,
	) {
		relay_substrate_client::guard::abort_on_spec_version_change(
			target_client.clone(),
			bp_kusama::VERSION.spec_version,
		);
		relay_substrate_client::guard::abort_when_account_balance_decreased(
			target_client.clone(),
			transaction_params.signer.public().into(),
			MAXIMAL_BALANCE_DECREASE_PER_DAY,
		);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::chains::kusama_headers_to_polkadot::tests::compute_maximal_balance_decrease_per_day;

	#[test]
	fn maximal_balance_decrease_per_day_is_sane() {
		// we expect Polkadot -> Kusama relay to be running in mandatory-headers-only mode
		// => we expect single header for every Polkadot session
		let maximal_balance_decrease = compute_maximal_balance_decrease_per_day::<
			bp_kusama::Balance,
			bp_kusama::WeightToFee,
		>(bp_polkadot::DAYS / bp_polkadot::SESSION_LENGTH + 1);
		assert!(
			MAXIMAL_BALANCE_DECREASE_PER_DAY >= maximal_balance_decrease,
			"Maximal expected loss per day {} is larger than hardcoded {}",
			maximal_balance_decrease,
			MAXIMAL_BALANCE_DECREASE_PER_DAY,
		);
	}
}
