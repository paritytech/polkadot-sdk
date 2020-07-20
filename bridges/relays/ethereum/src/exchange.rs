// Copyright 2019-2020 Parity Technologies (UK) Ltd.
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

//! Relaying proofs of exchange transactions.

use async_trait::async_trait;
use std::fmt::{Debug, Display};

/// Transaction proof pipeline.
pub trait TransactionProofPipeline {
	/// Name of the transaction proof source.
	const SOURCE_NAME: &'static str;
	/// Name of the transaction proof target.
	const TARGET_NAME: &'static str;

	/// Block hash type.
	type BlockHash: Display;
	/// Block number type.
	type BlockNumber: Display;
	/// Transaction hash type.
	type TransactionHash: Display;
	/// Transaction type.
	type Transaction;
	/// Transaction inclusion proof type.
	type TransactionProof;
}

/// Header id.
pub type HeaderId<P> = crate::sync_types::HeaderId<
	<P as TransactionProofPipeline>::BlockHash,
	<P as TransactionProofPipeline>::BlockNumber,
>;

/// Source client API.
#[async_trait]
pub trait SourceClient<P: TransactionProofPipeline> {
	/// Error type.
	type Error: Debug;

	/// Sleep until exchange-related data is (probably) updated.
	async fn tick(&self);
	/// Return **mined** transaction by its hash. May return `Ok(None)` if transaction is unknown to the source node.
	async fn transaction(
		&self,
		hash: &P::TransactionHash,
	) -> Result<Option<(HeaderId<P>, P::Transaction)>, Self::Error>;
	/// Prepare transaction proof.
	async fn transaction_proof(
		&self,
		header: &HeaderId<P>,
		transaction: P::Transaction,
	) -> Result<P::TransactionProof, Self::Error>;
}

/// Target client API.
#[async_trait]
pub trait TargetClient<P: TransactionProofPipeline> {
	/// Error type.
	type Error: Debug;

	/// Sleep until exchange-related data is (probably) updated.
	async fn tick(&self);
	/// Returns `Ok(true)` if header is known to the target node.
	async fn is_header_known(&self, id: &HeaderId<P>) -> Result<bool, Self::Error>;
	/// Returns `Ok(true)` if header is finalized by the target node.
	async fn is_header_finalized(&self, id: &HeaderId<P>) -> Result<bool, Self::Error>;
	/// Submits transaction proof to the target node.
	async fn submit_transaction_proof(&self, proof: P::TransactionProof) -> Result<(), Self::Error>;
}

/// Relay single transaction proof.
pub async fn relay_single_transaction_proof<P: TransactionProofPipeline>(
	source_client: &impl SourceClient<P>,
	target_client: &impl TargetClient<P>,
	source_tx_hash: P::TransactionHash,
) -> Result<(), String> {
	// wait for transaction and header on source node
	let (source_header_id, source_tx) = wait_transaction_mined(source_client, &source_tx_hash).await?;
	let transaction_proof = source_client
		.transaction_proof(&source_header_id, source_tx)
		.await
		.map_err(|err| {
			format!(
				"Error building transaction {} proof on {} node: {:?}",
				source_tx_hash,
				P::SOURCE_NAME,
				err,
			)
		})?;

	// wait for transaction and header on target node
	wait_header_imported(target_client, &source_header_id).await?;
	wait_header_finalized(target_client, &source_header_id).await?;

	// and finally - submit transaction proof to target node
	target_client
		.submit_transaction_proof(transaction_proof)
		.await
		.map_err(|err| {
			format!(
				"Error submitting transaction {} proof to {} node: {:?}",
				source_tx_hash,
				P::TARGET_NAME,
				err,
			)
		})
}

/// Wait until transaction is mined by source node.
async fn wait_transaction_mined<P: TransactionProofPipeline>(
	source_client: &impl SourceClient<P>,
	source_tx_hash: &P::TransactionHash,
) -> Result<(HeaderId<P>, P::Transaction), String> {
	loop {
		let source_header_and_tx = source_client.transaction(&source_tx_hash).await.map_err(|err| {
			format!(
				"Error retrieving transaction {} from {} node: {:?}",
				source_tx_hash,
				P::SOURCE_NAME,
				err,
			)
		})?;
		match source_header_and_tx {
			Some((source_header_id, source_tx)) => {
				log::info!(
					target: "bridge",
					"Transaction {} is retrieved from {} node. Continuing...",
					source_tx_hash,
					P::SOURCE_NAME,
				);

				return Ok((source_header_id, source_tx));
			}
			None => {
				log::info!(
					target: "bridge",
					"Waiting for transaction {} to be mined by {} node...",
					source_tx_hash,
					P::SOURCE_NAME,
				);

				source_client.tick().await;
			}
		}
	}
}

/// Wait until target node imports required header.
async fn wait_header_imported<P: TransactionProofPipeline>(
	target_client: &impl TargetClient<P>,
	source_header_id: &HeaderId<P>,
) -> Result<(), String> {
	loop {
		let is_header_known = target_client.is_header_known(&source_header_id).await.map_err(|err| {
			format!(
				"Failed to check existence of header {}/{} on {} node: {:?}",
				source_header_id.0,
				source_header_id.1,
				P::TARGET_NAME,
				err,
			)
		})?;
		match is_header_known {
			true => {
				log::info!(
					target: "bridge",
					"Header {}/{} is known to {} node. Continuing.",
					source_header_id.0,
					source_header_id.1,
					P::TARGET_NAME,
				);

				return Ok(());
			}
			false => {
				log::info!(
					target: "bridge",
					"Waiting for header {}/{} to be imported by {} node...",
					source_header_id.0,
					source_header_id.1,
					P::TARGET_NAME,
				);

				target_client.tick().await;
			}
		}
	}
}

/// Wait until target node finalizes required header.
async fn wait_header_finalized<P: TransactionProofPipeline>(
	target_client: &impl TargetClient<P>,
	source_header_id: &HeaderId<P>,
) -> Result<(), String> {
	loop {
		let is_header_finalized = target_client
			.is_header_finalized(&source_header_id)
			.await
			.map_err(|err| {
				format!(
					"Failed to check finality of header {}/{} on {} node: {:?}",
					source_header_id.0,
					source_header_id.1,
					P::TARGET_NAME,
					err,
				)
			})?;
		match is_header_finalized {
			true => {
				log::info!(
					target: "bridge",
					"Header {}/{} is finalizd by {} node. Continuing.",
					source_header_id.0,
					source_header_id.1,
					P::TARGET_NAME,
				);

				return Ok(());
			}
			false => {
				log::info!(
					target: "bridge",
					"Waiting for header {}/{} to be finalized by {} node...",
					source_header_id.0,
					source_header_id.1,
					P::TARGET_NAME,
				);

				target_client.tick().await;
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::sync_types::HeaderId;

	use parking_lot::Mutex;

	fn test_header_id() -> TestHeaderId {
		HeaderId(100, 100)
	}

	fn test_transaction_hash() -> TestTransactionHash {
		200
	}

	fn test_transaction() -> TestTransaction {
		300
	}

	fn test_transaction_proof() -> TestTransactionProof {
		400
	}

	type TestError = u64;
	type TestBlockNumber = u64;
	type TestBlockHash = u64;
	type TestTransactionHash = u64;
	type TestTransaction = u64;
	type TestTransactionProof = u64;
	type TestHeaderId = HeaderId<TestBlockHash, TestBlockNumber>;

	struct TestTransactionProofPipeline;

	impl TransactionProofPipeline for TestTransactionProofPipeline {
		const SOURCE_NAME: &'static str = "TestSource";
		const TARGET_NAME: &'static str = "TestTarget";

		type BlockHash = TestBlockHash;
		type BlockNumber = TestBlockNumber;
		type TransactionHash = TestTransactionHash;
		type Transaction = TestTransaction;
		type TransactionProof = TestTransactionProof;
	}

	struct TestTransactionsSource {
		on_tick: Box<dyn Fn(&mut TestTransactionsSourceData) + Send + Sync>,
		data: Mutex<TestTransactionsSourceData>,
	}

	struct TestTransactionsSourceData {
		transaction: Result<Option<(TestHeaderId, TestTransaction)>, TestError>,
		transaction_proof: Result<TestTransactionProof, TestError>,
	}

	impl TestTransactionsSource {
		fn new(on_tick: Box<dyn Fn(&mut TestTransactionsSourceData) + Send + Sync>) -> Self {
			Self {
				on_tick,
				data: Mutex::new(TestTransactionsSourceData {
					transaction: Ok(Some((test_header_id(), test_transaction()))),
					transaction_proof: Ok(test_transaction_proof()),
				}),
			}
		}
	}

	#[async_trait]
	impl SourceClient<TestTransactionProofPipeline> for TestTransactionsSource {
		type Error = TestError;

		async fn tick(&self) {
			(self.on_tick)(&mut *self.data.lock())
		}

		async fn transaction(
			&self,
			_: &TestTransactionHash,
		) -> Result<Option<(TestHeaderId, TestTransaction)>, TestError> {
			self.data.lock().transaction
		}

		async fn transaction_proof(
			&self,
			_: &TestHeaderId,
			_: TestTransaction,
		) -> Result<TestTransactionProof, TestError> {
			self.data.lock().transaction_proof
		}
	}

	struct TestTransactionsTarget {
		on_tick: Box<dyn Fn(&mut TestTransactionsTargetData) + Send + Sync>,
		data: Mutex<TestTransactionsTargetData>,
	}

	struct TestTransactionsTargetData {
		is_header_known: Result<bool, TestError>,
		is_header_finalized: Result<bool, TestError>,
		submitted_proofs: Vec<TestTransactionProof>,
	}

	impl TestTransactionsTarget {
		fn new(on_tick: Box<dyn Fn(&mut TestTransactionsTargetData) + Send + Sync>) -> Self {
			Self {
				on_tick,
				data: Mutex::new(TestTransactionsTargetData {
					is_header_known: Ok(true),
					is_header_finalized: Ok(true),
					submitted_proofs: Vec::new(),
				}),
			}
		}
	}

	#[async_trait]
	impl TargetClient<TestTransactionProofPipeline> for TestTransactionsTarget {
		type Error = TestError;

		async fn tick(&self) {
			(self.on_tick)(&mut *self.data.lock())
		}

		async fn is_header_known(&self, _: &TestHeaderId) -> Result<bool, TestError> {
			self.data.lock().is_header_known
		}

		async fn is_header_finalized(&self, _: &TestHeaderId) -> Result<bool, TestError> {
			self.data.lock().is_header_finalized
		}

		async fn submit_transaction_proof(&self, proof: TestTransactionProof) -> Result<(), TestError> {
			self.data.lock().submitted_proofs.push(proof);
			Ok(())
		}
	}

	fn ensure_success(source: TestTransactionsSource, target: TestTransactionsTarget) {
		assert_eq!(
			async_std::task::block_on(relay_single_transaction_proof(
				&source,
				&target,
				test_transaction_hash(),
			)),
			Ok(()),
		);
		assert_eq!(target.data.lock().submitted_proofs, vec![test_transaction_proof()],);
	}

	fn ensure_failure(source: TestTransactionsSource, target: TestTransactionsTarget) {
		assert!(async_std::task::block_on(relay_single_transaction_proof(
			&source,
			&target,
			test_transaction_hash(),
		))
		.is_err(),);
		assert!(target.data.lock().submitted_proofs.is_empty());
	}

	#[test]
	fn ready_transaction_proof_relayed_immediately() {
		let source = TestTransactionsSource::new(Box::new(|_| unreachable!("no ticks allowed")));
		let target = TestTransactionsTarget::new(Box::new(|_| unreachable!("no ticks allowed")));
		ensure_success(source, target)
	}

	#[test]
	fn relay_transaction_proof_waits_for_transaction_to_be_mined() {
		let source = TestTransactionsSource::new(Box::new(|source_data| {
			assert_eq!(source_data.transaction, Ok(None));
			source_data.transaction = Ok(Some((test_header_id(), test_transaction())));
		}));
		let target = TestTransactionsTarget::new(Box::new(|_| unreachable!("no ticks allowed")));

		// transaction is not yet mined, but will be available after first wait (tick)
		source.data.lock().transaction = Ok(None);

		ensure_success(source, target)
	}

	#[test]
	fn relay_transaction_fails_when_transaction_retrieval_fails() {
		let source = TestTransactionsSource::new(Box::new(|_| unreachable!("no ticks allowed")));
		let target = TestTransactionsTarget::new(Box::new(|_| unreachable!("no ticks allowed")));

		source.data.lock().transaction = Err(0);

		ensure_failure(source, target)
	}

	#[test]
	fn relay_transaction_fails_when_proof_retrieval_fails() {
		let source = TestTransactionsSource::new(Box::new(|_| unreachable!("no ticks allowed")));
		let target = TestTransactionsTarget::new(Box::new(|_| unreachable!("no ticks allowed")));

		source.data.lock().transaction_proof = Err(0);

		ensure_failure(source, target)
	}

	#[test]
	fn relay_transaction_proof_waits_for_header_to_be_imported() {
		let source = TestTransactionsSource::new(Box::new(|_| unreachable!("no ticks allowed")));
		let target = TestTransactionsTarget::new(Box::new(|target_data| {
			assert_eq!(target_data.is_header_known, Ok(false));
			target_data.is_header_known = Ok(true);
		}));

		// header is not yet imported, but will be available after first wait (tick)
		target.data.lock().is_header_known = Ok(false);

		ensure_success(source, target)
	}

	#[test]
	fn relay_transaction_proof_fails_when_is_header_known_fails() {
		let source = TestTransactionsSource::new(Box::new(|_| unreachable!("no ticks allowed")));
		let target = TestTransactionsTarget::new(Box::new(|_| unreachable!("no ticks allowed")));

		target.data.lock().is_header_known = Err(0);

		ensure_failure(source, target)
	}

	#[test]
	fn relay_transaction_proof_waits_for_header_to_be_finalized() {
		let source = TestTransactionsSource::new(Box::new(|_| unreachable!("no ticks allowed")));
		let target = TestTransactionsTarget::new(Box::new(|target_data| {
			assert_eq!(target_data.is_header_finalized, Ok(false));
			target_data.is_header_finalized = Ok(true);
		}));

		// header is not yet finalized, but will be available after first wait (tick)
		target.data.lock().is_header_finalized = Ok(false);

		ensure_success(source, target)
	}

	#[test]
	fn relay_transaction_proof_fails_when_is_header_finalized_fails() {
		let source = TestTransactionsSource::new(Box::new(|_| unreachable!("no ticks allowed")));
		let target = TestTransactionsTarget::new(Box::new(|_| unreachable!("no ticks allowed")));

		target.data.lock().is_header_finalized = Err(0);

		ensure_failure(source, target)
	}
}
