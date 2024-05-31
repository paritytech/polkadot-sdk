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

//! Tests for finality synchronization loop.

#![cfg(test)]

use crate::{
	base::SourceClientBase,
	finality_loop::{SourceClient, TargetClient},
	FinalityPipeline, FinalitySyncPipeline, SourceHeader,
};

use async_trait::async_trait;
use bp_header_chain::{FinalityProof, GrandpaConsensusLogReader};
use futures::{Stream, StreamExt};
use parking_lot::Mutex;
use relay_utils::{
	relay_loop::Client as RelayClient, HeaderId, MaybeConnectionError, TrackedTransactionStatus,
	TransactionTracker,
};
use std::{collections::HashMap, pin::Pin, sync::Arc};

type IsMandatory = bool;
pub type TestNumber = u64;
type TestHash = u64;

#[derive(Clone, Debug)]
pub struct TestTransactionTracker(pub TrackedTransactionStatus<HeaderId<TestHash, TestNumber>>);

impl Default for TestTransactionTracker {
	fn default() -> TestTransactionTracker {
		TestTransactionTracker(TrackedTransactionStatus::Finalized(Default::default()))
	}
}

#[async_trait]
impl TransactionTracker for TestTransactionTracker {
	type HeaderId = HeaderId<TestHash, TestNumber>;

	async fn wait(self) -> TrackedTransactionStatus<HeaderId<TestHash, TestNumber>> {
		self.0
	}
}

#[derive(Debug, Clone)]
pub enum TestError {
	NonConnection,
}

impl MaybeConnectionError for TestError {
	fn is_connection_error(&self) -> bool {
		false
	}
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestFinalitySyncPipeline;

impl FinalityPipeline for TestFinalitySyncPipeline {
	const SOURCE_NAME: &'static str = "TestSource";
	const TARGET_NAME: &'static str = "TestTarget";

	type Hash = TestHash;
	type Number = TestNumber;
	type FinalityProof = TestFinalityProof;
}

impl FinalitySyncPipeline for TestFinalitySyncPipeline {
	type ConsensusLogReader = GrandpaConsensusLogReader<TestNumber>;
	type Header = TestSourceHeader;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestSourceHeader(pub IsMandatory, pub TestNumber, pub TestHash);

impl SourceHeader<TestHash, TestNumber, GrandpaConsensusLogReader<TestNumber>>
	for TestSourceHeader
{
	fn hash(&self) -> TestHash {
		self.2
	}

	fn number(&self) -> TestNumber {
		self.1
	}

	fn is_mandatory(&self) -> bool {
		self.0
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestFinalityProof(pub TestNumber);

impl FinalityProof<TestHash, TestNumber> for TestFinalityProof {
	fn target_header_hash(&self) -> TestHash {
		Default::default()
	}

	fn target_header_number(&self) -> TestNumber {
		self.0
	}
}

#[derive(Debug, Clone, Default)]
pub struct ClientsData {
	pub source_best_block_number: TestNumber,
	pub source_headers: HashMap<TestNumber, (TestSourceHeader, Option<TestFinalityProof>)>,
	pub source_proofs: Vec<TestFinalityProof>,

	pub target_best_block_id: HeaderId<TestHash, TestNumber>,
	pub target_headers: Vec<(TestSourceHeader, TestFinalityProof)>,
	pub target_transaction_tracker: TestTransactionTracker,
}

#[derive(Clone)]
pub struct TestSourceClient {
	pub on_method_call: Arc<dyn Fn(&mut ClientsData) + Send + Sync>,
	pub data: Arc<Mutex<ClientsData>>,
}

#[async_trait]
impl RelayClient for TestSourceClient {
	type Error = TestError;

	async fn reconnect(&mut self) -> Result<(), TestError> {
		unreachable!()
	}
}

#[async_trait]
impl SourceClientBase<TestFinalitySyncPipeline> for TestSourceClient {
	type FinalityProofsStream = Pin<Box<dyn Stream<Item = TestFinalityProof> + 'static + Send>>;

	async fn finality_proofs(&self) -> Result<Self::FinalityProofsStream, TestError> {
		let mut data = self.data.lock();
		(self.on_method_call)(&mut data);
		Ok(futures::stream::iter(data.source_proofs.clone()).boxed())
	}
}

#[async_trait]
impl SourceClient<TestFinalitySyncPipeline> for TestSourceClient {
	async fn best_finalized_block_number(&self) -> Result<TestNumber, TestError> {
		let mut data = self.data.lock();
		(self.on_method_call)(&mut data);
		Ok(data.source_best_block_number)
	}

	async fn header_and_finality_proof(
		&self,
		number: TestNumber,
	) -> Result<(TestSourceHeader, Option<TestFinalityProof>), TestError> {
		let mut data = self.data.lock();
		(self.on_method_call)(&mut data);
		data.source_headers.get(&number).cloned().ok_or(TestError::NonConnection)
	}
}

#[derive(Clone)]
pub struct TestTargetClient {
	pub on_method_call: Arc<dyn Fn(&mut ClientsData) + Send + Sync>,
	pub data: Arc<Mutex<ClientsData>>,
}

#[async_trait]
impl RelayClient for TestTargetClient {
	type Error = TestError;

	async fn reconnect(&mut self) -> Result<(), TestError> {
		unreachable!()
	}
}

#[async_trait]
impl TargetClient<TestFinalitySyncPipeline> for TestTargetClient {
	type TransactionTracker = TestTransactionTracker;

	async fn best_finalized_source_block_id(
		&self,
	) -> Result<HeaderId<TestHash, TestNumber>, TestError> {
		let mut data = self.data.lock();
		(self.on_method_call)(&mut data);
		Ok(data.target_best_block_id)
	}

	async fn free_source_headers_interval(&self) -> Result<Option<TestNumber>, TestError> {
		Ok(Some(3))
	}

	async fn submit_finality_proof(
		&self,
		header: TestSourceHeader,
		proof: TestFinalityProof,
		_is_free_execution_expected: bool,
	) -> Result<TestTransactionTracker, TestError> {
		let mut data = self.data.lock();
		(self.on_method_call)(&mut data);
		data.target_best_block_id = HeaderId(header.number(), header.hash());
		data.target_headers.push((header, proof));
		(self.on_method_call)(&mut data);
		Ok(data.target_transaction_tracker.clone())
	}
}
