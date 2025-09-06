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

//! Initialize Substrate -> Substrate finality bridge.
//!
//! Initialization is a transaction that calls `initialize()` function of the
//! finality pallet (GRANDPA/BEEFY/...). This transaction brings initial header
//! and authorities set from source to target chain. The finality sync starts
//! with this header.

use crate::{error::Error, finality_base::engine::Engine};
use sp_core::Pair;

use bp_runtime::HeaderIdOf;
use relay_substrate_client::{
	AccountKeyPairOf, Chain, ChainWithTransactions, Client, Error as SubstrateError,
	UnsignedTransaction,
};
use relay_utils::{TrackedTransactionStatus, TransactionTracker};
use sp_runtime::traits::Header as HeaderT;

/// Submit headers-bridge initialization transaction.
pub async fn initialize<
	E: Engine<SourceChain>,
	SourceChain: Chain,
	TargetChain: ChainWithTransactions,
	F,
>(
	source_client: impl Client<SourceChain>,
	target_client: impl Client<TargetChain>,
	target_signer: AccountKeyPairOf<TargetChain>,
	prepare_initialize_transaction: F,
	dry_run: bool,
) where
	F: FnOnce(
			TargetChain::Nonce,
			E::InitializationData,
		) -> Result<UnsignedTransaction<TargetChain>, SubstrateError>
		+ Send
		+ 'static,
	TargetChain::AccountId: From<<TargetChain::AccountKeyPair as Pair>::Public>,
{
	let result = do_initialize::<E, _, _, _>(
		source_client,
		target_client,
		target_signer,
		prepare_initialize_transaction,
		dry_run,
	)
	.await;

	match result {
		Ok(Some(tx_status)) => match tx_status {
			TrackedTransactionStatus::Lost => {
				tracing::error!(
					target: "bridge",
					source=%SourceChain::NAME,
					target=%TargetChain::NAME,
					?tx_status,
					"Failed to execute headers bridge initialization transaction."
				)
			},
			TrackedTransactionStatus::Finalized(_) => {
				tracing::info!(
					target: "bridge",
					source=%SourceChain::NAME,
					target=%TargetChain::NAME,
					?tx_status,
					"Successfully executed headers bridge initialization transaction."
				)
			},
		},
		Ok(None) => (),
		Err(err) => tracing::error!(
			target: "bridge",
			error=?err,
			source=%SourceChain::NAME,
			target=%TargetChain::NAME,
			"Failed to submit headers bridge initialization transaction"
		),
	}
}

/// Craft and submit initialization transaction, returning any error that may occur.
async fn do_initialize<
	E: Engine<SourceChain>,
	SourceChain: Chain,
	TargetChain: ChainWithTransactions,
	F,
>(
	source_client: impl Client<SourceChain>,
	target_client: impl Client<TargetChain>,
	target_signer: AccountKeyPairOf<TargetChain>,
	prepare_initialize_transaction: F,
	dry_run: bool,
) -> Result<
	Option<TrackedTransactionStatus<HeaderIdOf<TargetChain>>>,
	Error<SourceChain::Hash, <SourceChain::Header as HeaderT>::Number>,
>
where
	F: FnOnce(
			TargetChain::Nonce,
			E::InitializationData,
		) -> Result<UnsignedTransaction<TargetChain>, SubstrateError>
		+ Send
		+ 'static,
	TargetChain::AccountId: From<<TargetChain::AccountKeyPair as Pair>::Public>,
{
	let is_initialized = E::is_initialized(&target_client)
		.await
		.map_err(|e| Error::IsInitializedRetrieve(SourceChain::NAME, TargetChain::NAME, e))?;
	if is_initialized {
		tracing::info!(
			target: "bridge",
			source=%SourceChain::NAME,
			target=%TargetChain::NAME,
			"Headers bridge is already initialized. Skipping"
		);
		if !dry_run {
			return Ok(None)
		}
	}

	let initialization_data = E::prepare_initialization_data(source_client).await?;
	tracing::info!(
		target: "bridge",
		source=%SourceChain::NAME,
		target=%TargetChain::NAME,
		?initialization_data,
		"Prepared initialization data for headers bridge"
	);

	let tx_status = target_client
		.submit_and_watch_signed_extrinsic(&target_signer, move |_, transaction_nonce| {
			let tx = prepare_initialize_transaction(transaction_nonce, initialization_data);
			if dry_run {
				Err(SubstrateError::Custom(
					"Not submitting extrinsic in `dry-run` mode!".to_string(),
				))
			} else {
				tx
			}
		})
		.await
		.map_err(|err| Error::SubmitTransaction(TargetChain::NAME, err))?
		.wait()
		.await;

	Ok(Some(tx_status))
}
