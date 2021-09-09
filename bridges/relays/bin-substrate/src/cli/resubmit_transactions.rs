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

use crate::cli::{TargetConnectionParams, TargetSigningParams};

use codec::{Decode, Encode};
use num_traits::{One, Zero};
use relay_substrate_client::{BlockWithJustification, Chain, Client, Error as SubstrateError, TransactionSignScheme};
use relay_utils::FailedClient;
use sp_core::Bytes;
use sp_runtime::{
	traits::{Hash, Header as HeaderT},
	transaction_validity::TransactionPriority,
};
use structopt::StructOpt;
use strum::{EnumString, EnumVariantNames, VariantNames};

/// Start resubmit transactions process.
#[derive(StructOpt)]
pub struct ResubmitTransactions {
	/// A bridge instance to relay headers for.
	#[structopt(possible_values = RelayChain::VARIANTS, case_insensitive = true)]
	chain: RelayChain,
	#[structopt(flatten)]
	target: TargetConnectionParams,
	#[structopt(flatten)]
	target_sign: TargetSigningParams,
}

/// Chain, which transactions we're going to track && resubmit.
#[derive(Debug, EnumString, EnumVariantNames)]
#[strum(serialize_all = "kebab_case")]
pub enum RelayChain {
	Millau,
}

macro_rules! select_bridge {
	($bridge: expr, $generic: tt) => {
		match $bridge {
			RelayChain::Millau => {
				type Target = relay_millau_client::Millau;
				type TargetSign = relay_millau_client::Millau;

				const TIP_STEP: bp_millau::Balance = 1_000_000;
				const TIP_LIMIT: bp_millau::Balance = 1_000_000_000;

				const STALLED_BLOCKS: bp_millau::BlockNumber = 5;

				$generic
			}
		}
	};
}

impl ResubmitTransactions {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		select_bridge!(self.chain, {
			let relay_loop_name = format!("ResubmitTransactions{}", Target::NAME);
			let client = self.target.to_client::<Target>().await?;
			let key_pair = self.target_sign.to_keypair::<Target>()?;

			relay_utils::relay_loop((), client)
				.run(relay_loop_name, move |_, client, _| {
					run_until_connection_lost::<Target, TargetSign>(
						client,
						key_pair.clone(),
						Context {
							transaction: None,
							stalled_for: Zero::zero(),
							stalled_for_limit: STALLED_BLOCKS,
							tip_step: TIP_STEP,
							tip_limit: TIP_LIMIT,
						},
					)
				})
				.await
		})
	}
}

#[derive(Debug, Default)]
struct Context<C: Chain> {
	/// Hash of the (potentially) stalled transaction.
	transaction: Option<C::Hash>,
	/// This transaction is in pool for `stalled_for` wakeup intervals.
	stalled_for: C::BlockNumber,
	/// When `stalled_for` reaching this limit, transaction is considered stalled.
	stalled_for_limit: C::BlockNumber,
	/// Tip step interval.
	tip_step: C::Balance,
	/// Maximal tip.
	tip_limit: C::Balance,
}

impl<C: Chain> Context<C> {
	/// Return true if transaction has stalled.
	fn is_stalled(&self) -> bool {
		self.stalled_for >= self.stalled_for_limit
	}

	/// Forget stalled transaction.
	fn clear(mut self) -> Self {
		self.transaction = None;
		self.stalled_for = Zero::zero();
		self
	}

	/// Notice transaction from the transaction pool.
	fn notice_transaction(mut self, transaction: C::Hash) -> Self {
		if self.transaction == Some(transaction) {
			self.stalled_for += One::one();
		} else {
			self.transaction = Some(transaction);
			self.stalled_for = One::one();
		}
		self
	}
}

/// Run resubmit transactions loop.
async fn run_until_connection_lost<C: Chain, S: TransactionSignScheme<Chain = C>>(
	client: Client<C>,
	key_pair: S::AccountKeyPair,
	mut context: Context<C>,
) -> Result<(), FailedClient> {
	loop {
		async_std::task::sleep(C::AVERAGE_BLOCK_INTERVAL).await;

		let result = run_loop_iteration::<C, S>(client.clone(), key_pair.clone(), context).await;
		context = match result {
			Ok(context) => context,
			Err(error) => {
				log::error!(
					target: "bridge",
					"Resubmit {} transactions loop has failed with error: {:?}",
					C::NAME,
					error,
				);
				return Err(FailedClient::Target);
			}
		};
	}
}

/// Run single loop iteration.
async fn run_loop_iteration<C: Chain, S: TransactionSignScheme<Chain = C>>(
	client: Client<C>,
	key_pair: S::AccountKeyPair,
	context: Context<C>,
) -> Result<Context<C>, SubstrateError> {
	let original_transaction = match lookup_signer_transaction::<C, S>(&client, &key_pair).await? {
		Some(original_transaction) => original_transaction,
		None => {
			log::trace!(target: "bridge", "No {} transactions from required signer in the txpool", C::NAME);
			return Ok(context);
		}
	};
	let original_transaction_hash = C::Hasher::hash(&original_transaction.encode());
	let context = context.notice_transaction(original_transaction_hash);

	if !context.is_stalled() {
		log::trace!(
			target: "bridge",
			"{} transaction {:?} is not yet stalled ({:?}/{:?})",
			C::NAME,
			context.transaction,
			context.stalled_for,
			context.stalled_for_limit,
		);
		return Ok(context);
	}

	let (best_block, target_priority) = match read_previous_best_priority::<C, S>(&client).await? {
		Some((best_block, target_priority)) => (best_block, target_priority),
		None => {
			log::trace!(target: "bridge", "Failed to read priority of best {} transaction in its best block", C::NAME);
			return Ok(context);
		}
	};

	let (is_updated, updated_transaction) = select_transaction_tip::<C, S>(
		&client,
		&key_pair,
		best_block,
		original_transaction,
		context.tip_step,
		context.tip_limit,
		target_priority,
	)
	.await?;

	if !is_updated {
		log::trace!(target: "bridge", "{} transaction tip can not be updated. Reached limit?", C::NAME);
		return Ok(context);
	}

	let updated_transaction = updated_transaction.encode();
	let updated_transaction_hash = C::Hasher::hash(&updated_transaction);
	client.submit_unsigned_extrinsic(Bytes(updated_transaction)).await?;

	log::info!(
		target: "bridge",
		"Replaced {} transaction {} with {} in txpool",
		C::NAME,
		original_transaction_hash,
		updated_transaction_hash,
	);

	Ok(context.clear())
}

/// Search transaction pool for transaction, signed by given key pair.
async fn lookup_signer_transaction<C: Chain, S: TransactionSignScheme<Chain = C>>(
	client: &Client<C>,
	key_pair: &S::AccountKeyPair,
) -> Result<Option<S::SignedTransaction>, SubstrateError> {
	let pending_transactions = client.pending_extrinsics().await?;
	for pending_transaction in pending_transactions {
		let pending_transaction = S::SignedTransaction::decode(&mut &pending_transaction.0[..])
			.map_err(SubstrateError::ResponseParseFailed)?;
		if !S::is_signed_by(key_pair, &pending_transaction) {
			continue;
		}

		return Ok(Some(pending_transaction));
	}

	Ok(None)
}

/// Read priority of best signed transaction of previous block.
async fn read_previous_best_priority<C: Chain, S: TransactionSignScheme<Chain = C>>(
	client: &Client<C>,
) -> Result<Option<(C::Hash, TransactionPriority)>, SubstrateError> {
	let best_header = client.best_header().await?;
	let best_header_hash = best_header.hash();
	let best_block = client.get_block(Some(best_header_hash)).await?;
	let best_transaction = best_block
		.extrinsics()
		.iter()
		.filter_map(|xt| S::SignedTransaction::decode(&mut &xt[..]).ok())
		.find(|xt| S::is_signed(xt));
	match best_transaction {
		Some(best_transaction) => Ok(Some((
			best_header_hash,
			client
				.validate_transaction(*best_header.parent_hash(), best_transaction)
				.await??
				.priority,
		))),
		None => Ok(None),
	}
}

/// Try to find appropriate tip for transaction so that its priority is larger than given.
async fn select_transaction_tip<C: Chain, S: TransactionSignScheme<Chain = C>>(
	client: &Client<C>,
	key_pair: &S::AccountKeyPair,
	at_block: C::Hash,
	tx: S::SignedTransaction,
	tip_step: C::Balance,
	tip_limit: C::Balance,
	target_priority: TransactionPriority,
) -> Result<(bool, S::SignedTransaction), SubstrateError> {
	let stx = format!("{:?}", tx);
	let mut current_priority = client.validate_transaction(at_block, tx.clone()).await??.priority;
	let mut unsigned_tx = S::parse_transaction(tx)
		.ok_or_else(|| SubstrateError::Custom(format!("Failed to parse {} transaction {}", C::NAME, stx,)))?;
	let old_tip = unsigned_tx.tip;

	while current_priority < target_priority {
		let next_tip = unsigned_tx.tip + tip_step;
		if next_tip > tip_limit {
			break;
		}

		log::trace!(
			target: "bridge",
			"{} transaction priority with tip={:?}: {}. Target priority: {}",
			C::NAME,
			unsigned_tx.tip,
			current_priority,
			target_priority,
		);

		unsigned_tx.tip = next_tip;
		current_priority = client
			.validate_transaction(
				at_block,
				S::sign_transaction(
					*client.genesis_hash(),
					key_pair,
					relay_substrate_client::TransactionEra::immortal(),
					unsigned_tx.clone(),
				),
			)
			.await??
			.priority;
	}

	log::debug!(
		target: "bridge",
		"{} transaction tip has changed from {:?} to {:?}",
		C::NAME,
		old_tip,
		unsigned_tx.tip,
	);

	Ok((
		old_tip != unsigned_tx.tip,
		S::sign_transaction(
			*client.genesis_hash(),
			key_pair,
			relay_substrate_client::TransactionEra::immortal(),
			unsigned_tx,
		),
	))
}

#[cfg(test)]
mod tests {
	use super::*;
	use relay_rialto_client::Rialto;

	#[test]
	fn context_works() {
		let mut context: Context<Rialto> = Context {
			transaction: None,
			stalled_for: Zero::zero(),
			stalled_for_limit: 3,
			tip_step: 100,
			tip_limit: 1000,
		};

		// when transaction is noticed 2/3 times, it isn't stalled
		context = context.notice_transaction(Default::default());
		assert!(!context.is_stalled());
		context = context.notice_transaction(Default::default());
		assert!(!context.is_stalled());

		// when transaction is noticed for 3rd time in a row, it is considered stalled
		context = context.notice_transaction(Default::default());
		assert!(context.is_stalled());

		// and after we resubmit it, we forget previous transaction
		context = context.clear();
		assert_eq!(context.transaction, None);
		assert_eq!(context.stalled_for, 0);
	}
}
