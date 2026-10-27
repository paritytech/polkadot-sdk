//! Submitting a validator's own dealing to the aggregation parachain over
//! JSON-RPC.
//!
//! The dealing goes to a parachain node as an unsigned extrinsic
//! (`author_submitExtrinsic`). It is self-authenticating (its proof of
//! possession binds it to the dealer's BEEFY key), so the validator needs no
//! parachain account.
//!
//! The parachain only accepts a dealing once it has started the round of the
//! dealing's relay session, which lags the relay chain. A transaction pool bans
//! a transaction it rejected for a while, so a dealing submitted too early
//! could not be resubmitted in time. The submitter therefore first asks the
//! node to validate the dealing (`TaggedTransactionQueue_validate_transaction`,
//! the check the pool runs, which bans nothing) and only submits it once it is
//! valid, retrying while it is not valid *yet* or the node is unreachable.

use std::{sync::Arc, time::Duration};

use codec::{Decode, Encode};
use log::{debug, info, warn};
use sp_core::{traits::SpawnNamed, Bytes, H256};
use sp_runtime::transaction_validity::{
	InvalidTransaction, TransactionSource, TransactionValidity, TransactionValidityError,
};
use substrate_rpc_client::{rpc_params, ws_client, AuthorApi, ClientT};

use crate::{worker::DealingSubmitter, LOG_TARGET};

/// Encodes a dealing (relay session index, serialized transcript) as an
/// unsigned extrinsic of the aggregation parachain.
pub type DealingEncoder = Arc<dyn Fn(u32, Vec<u8>) -> Vec<u8> + Send + Sync>;

/// How often a submission is retried.
#[derive(Clone, Copy, Debug)]
pub struct RetryPolicy {
	/// Pause between attempts.
	pub interval: Duration,
	/// Maximum number of attempts.
	pub attempts: u32,
}

impl Default for RetryPolicy {
	fn default() -> Self {
		Self { interval: Duration::from_secs(3), attempts: 100 }
	}
}

/// The outcome of one submission attempt that reached the node.
enum Attempt {
	/// The dealing is in the node's transaction pool.
	Submitted(H256),
	/// The dealing is not valid yet (the round has not started).
	NotYet,
	/// The node rejects the dealing for good (e.g. its round is over).
	Rejected(String),
}

/// A [`DealingSubmitter`] that sends each dealing, encoded by `encode`, to the
/// parachain node at `url` (a WebSocket JSON-RPC endpoint) as soon as the node
/// considers it valid, retrying per `retry`.
pub fn rpc_dealing_submitter(
	url: String,
	encode: DealingEncoder,
	spawner: Arc<dyn SpawnNamed>,
	retry: RetryPolicy,
) -> DealingSubmitter {
	Arc::new(move |session_index, transcript_bytes| {
		let url = url.clone();
		let extrinsic = Bytes(encode(session_index, transcript_bytes));
		spawner.spawn(
			"dkg-submit-dealing",
			Some("dkg"),
			Box::pin(async move {
				for attempt in 1..=retry.attempts {
					match try_submit(&url, &extrinsic).await {
						Ok(Attempt::Submitted(hash)) => {
							info!(
								target: LOG_TARGET,
								"🎲 DKG: submitted dealing for session {session_index} to {url}: {hash:?}"
							);
							return;
						},
						Ok(Attempt::Rejected(reason)) => {
							warn!(
								target: LOG_TARGET,
								"🎲 DKG: {url} rejects the dealing for session {session_index}: {reason}"
							);
							return;
						},
						Ok(Attempt::NotYet) => debug!(
							target: LOG_TARGET,
							"🎲 DKG: dealing for session {session_index} not valid yet at {url} \
							 (attempt {attempt})"
						),
						Err(e) => debug!(
							target: LOG_TARGET,
							"🎲 DKG: submitting dealing for session {session_index} to {url} \
							 failed (attempt {attempt}): {e}"
						),
					}
					let _ = wasm_timer::Delay::new(retry.interval).await;
				}
				warn!(
					target: LOG_TARGET,
					"🎲 DKG: gave up submitting the dealing for session {session_index} to {url}"
				);
			}),
		);
	})
}

/// Validates `extrinsic` at the node's best block and submits it if valid.
async fn try_submit(url: &str, extrinsic: &Bytes) -> Result<Attempt, String> {
	let client = ws_client(url).await?;

	let best: Option<H256> = client
		.request("chain_getBlockHash", rpc_params![])
		.await
		.map_err(|e| e.to_string())?;
	let best = best.ok_or("node has no best block")?;
	// `validate_transaction(source, tx, block_hash)`; the opaque extrinsic
	// encodes as the extrinsic's own encoding.
	let call_data =
		Bytes([TransactionSource::External.encode(), extrinsic.0.clone(), best.encode()].concat());
	let validity: Bytes = client
		.request(
			"state_call",
			rpc_params!["TaggedTransactionQueue_validate_transaction", call_data, best],
		)
		.await
		.map_err(|e| e.to_string())?;
	match TransactionValidity::decode(&mut &validity.0[..]).map_err(|e| e.to_string())? {
		Ok(_) => {},
		Err(TransactionValidityError::Invalid(InvalidTransaction::Future)) |
		Err(TransactionValidityError::Unknown(_)) => return Ok(Attempt::NotYet),
		Err(e) => return Ok(Attempt::Rejected(format!("{e:?}"))),
	}

	match AuthorApi::<H256, H256>::submit_extrinsic(&client, extrinsic.clone()).await {
		Ok(hash) => Ok(Attempt::Submitted(hash)),
		// The pool already holds it (e.g. a previous attempt timed out).
		Err(e) if e.to_string().contains("Already Imported") => {
			Ok(Attempt::Submitted(H256::zero()))
		},
		Err(e) => Err(e.to_string()),
	}
}
