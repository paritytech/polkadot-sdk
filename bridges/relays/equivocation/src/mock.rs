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

#![cfg(test)]

use crate::{EquivocationDetectionPipeline, HeaderFinalityInfo, SourceClient, TargetClient};
use async_trait::async_trait;
use bp_header_chain::{FinalityProof, FindEquivocations};
use finality_relay::{FinalityPipeline, SourceClientBase};
use futures::{Stream, StreamExt};
use relay_utils::{
	relay_loop::Client as RelayClient, HeaderId, MaybeConnectionError, TrackedTransactionStatus,
	TransactionTracker,
};
use std::{
	collections::HashMap,
	pin::Pin,
	sync::{Arc, Mutex},
	time::Duration,
};

pub type TestSourceHashAndNumber = u64;
pub type TestTargetNumber = u64;
pub type TestEquivocationProof = &'static str;

pub const TEST_RECONNECT_DELAY: Duration = Duration::from_secs(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestFinalityProof(pub TestSourceHashAndNumber, pub Vec<TestEquivocationProof>);

impl FinalityProof<TestSourceHashAndNumber, TestSourceHashAndNumber> for TestFinalityProof {
	fn target_header_hash(&self) -> TestSourceHashAndNumber {
		self.0
	}

	fn target_header_number(&self) -> TestSourceHashAndNumber {
		self.0
	}
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestEquivocationDetectionPipeline;

impl FinalityPipeline for TestEquivocationDetectionPipeline {
	const SOURCE_NAME: &'static str = "TestSource";
	const TARGET_NAME: &'static str = "TestTarget";

	type Hash = TestSourceHashAndNumber;
	type Number = TestSourceHashAndNumber;
	type FinalityProof = TestFinalityProof;
}

#[derive(Clone, Debug, PartialEq)]
pub struct TestFinalityVerificationContext {
	pub check_equivocations: bool,
}

pub struct TestEquivocationsFinder;

impl FindEquivocations<TestFinalityProof, TestFinalityVerificationContext, TestEquivocationProof>
	for TestEquivocationsFinder
{
	type Error = ();

	fn find_equivocations(
		verification_context: &TestFinalityVerificationContext,
		synced_proof: &TestFinalityProof,
		source_proofs: &[TestFinalityProof],
	) -> Result<Vec<TestEquivocationProof>, Self::Error> {
		if verification_context.check_equivocations {
			// Get the equivocations from the source proofs, in order to make sure
			// that they are correctly provided.
			if let Some(proof) = source_proofs.iter().find(|proof| proof.0 == synced_proof.0) {
				return Ok(proof.1.clone())
			}
		}

		Ok(vec![])
	}
}

impl EquivocationDetectionPipeline for TestEquivocationDetectionPipeline {
	type TargetNumber = TestTargetNumber;
	type FinalityVerificationContext = TestFinalityVerificationContext;
	type EquivocationProof = TestEquivocationProof;
	type EquivocationsFinder = TestEquivocationsFinder;
}

#[derive(Debug, Clone)]
pub enum TestClientError {
	Connection,
	NonConnection,
}

impl MaybeConnectionError for TestClientError {
	fn is_connection_error(&self) -> bool {
		match self {
			TestClientError::Connection => true,
			TestClientError::NonConnection => false,
		}
	}
}

#[derive(Clone)]
pub struct TestSourceClient {
	pub num_reconnects: u32,
	pub finality_proofs: Arc<Mutex<Vec<TestFinalityProof>>>,
	pub reported_equivocations:
		Arc<Mutex<HashMap<TestSourceHashAndNumber, Vec<TestEquivocationProof>>>>,
}

impl Default for TestSourceClient {
	fn default() -> Self {
		Self {
			num_reconnects: 0,
			finality_proofs: Arc::new(Mutex::new(vec![])),
			reported_equivocations: Arc::new(Mutex::new(Default::default())),
		}
	}
}

#[async_trait]
impl RelayClient for TestSourceClient {
	type Error = TestClientError;

	async fn reconnect(&mut self) -> Result<(), Self::Error> {
		self.num_reconnects += 1;

		Ok(())
	}
}

#[async_trait]
impl SourceClientBase<TestEquivocationDetectionPipeline> for TestSourceClient {
	type FinalityProofsStream = Pin<Box<dyn Stream<Item = TestFinalityProof> + 'static + Send>>;

	async fn finality_proofs(&self) -> Result<Self::FinalityProofsStream, Self::Error> {
		let finality_proofs = std::mem::take(&mut *self.finality_proofs.lock().unwrap());
		Ok(futures::stream::iter(finality_proofs).boxed())
	}
}

#[derive(Clone, Debug)]
pub struct TestTransactionTracker(
	pub TrackedTransactionStatus<HeaderId<TestSourceHashAndNumber, TestSourceHashAndNumber>>,
);

impl Default for TestTransactionTracker {
	fn default() -> TestTransactionTracker {
		TestTransactionTracker(TrackedTransactionStatus::Finalized(Default::default()))
	}
}

#[async_trait]
impl TransactionTracker for TestTransactionTracker {
	type HeaderId = HeaderId<TestSourceHashAndNumber, TestSourceHashAndNumber>;

	async fn wait(
		self,
	) -> TrackedTransactionStatus<HeaderId<TestSourceHashAndNumber, TestSourceHashAndNumber>> {
		self.0
	}
}

#[async_trait]
impl SourceClient<TestEquivocationDetectionPipeline> for TestSourceClient {
	type TransactionTracker = TestTransactionTracker;

	async fn report_equivocation(
		&self,
		at: TestSourceHashAndNumber,
		equivocation: TestEquivocationProof,
	) -> Result<Self::TransactionTracker, Self::Error> {
		self.reported_equivocations
			.lock()
			.unwrap()
			.entry(at)
			.or_default()
			.push(equivocation);

		Ok(TestTransactionTracker::default())
	}
}

#[derive(Clone)]
pub struct TestTargetClient {
	pub num_reconnects: u32,
	pub best_finalized_header_number:
		Arc<dyn Fn() -> Result<TestTargetNumber, TestClientError> + Send + Sync>,
	pub best_synced_header_hash:
		HashMap<TestTargetNumber, Result<Option<TestSourceHashAndNumber>, TestClientError>>,
	pub finality_verification_context:
		HashMap<TestTargetNumber, Result<TestFinalityVerificationContext, TestClientError>>,
	pub synced_headers_finality_info: HashMap<
		TestTargetNumber,
		Result<Vec<HeaderFinalityInfo<TestEquivocationDetectionPipeline>>, TestClientError>,
	>,
}

impl Default for TestTargetClient {
	fn default() -> Self {
		Self {
			num_reconnects: 0,
			best_finalized_header_number: Arc::new(|| Ok(0)),
			best_synced_header_hash: Default::default(),
			finality_verification_context: Default::default(),
			synced_headers_finality_info: Default::default(),
		}
	}
}

#[async_trait]
impl RelayClient for TestTargetClient {
	type Error = TestClientError;

	async fn reconnect(&mut self) -> Result<(), Self::Error> {
		self.num_reconnects += 1;

		Ok(())
	}
}

#[async_trait]
impl TargetClient<TestEquivocationDetectionPipeline> for TestTargetClient {
	async fn best_finalized_header_number(&self) -> Result<TestTargetNumber, Self::Error> {
		(self.best_finalized_header_number)()
	}

	async fn best_synced_header_hash(
		&self,
		at: TestTargetNumber,
	) -> Result<Option<TestSourceHashAndNumber>, Self::Error> {
		self.best_synced_header_hash
			.get(&at)
			.unwrap_or(&Err(TestClientError::NonConnection))
			.clone()
	}

	async fn finality_verification_context(
		&self,
		at: TestTargetNumber,
	) -> Result<TestFinalityVerificationContext, Self::Error> {
		self.finality_verification_context
			.get(&at)
			.unwrap_or(&Err(TestClientError::NonConnection))
			.clone()
	}

	async fn synced_headers_finality_info(
		&self,
		at: TestTargetNumber,
	) -> Result<Vec<HeaderFinalityInfo<TestEquivocationDetectionPipeline>>, Self::Error> {
		self.synced_headers_finality_info
			.get(&at)
			.unwrap_or(&Err(TestClientError::NonConnection))
			.clone()
	}
}

pub fn new_header_finality_info(
	source_hdr: TestSourceHashAndNumber,
	check_following_equivocations: Option<bool>,
) -> HeaderFinalityInfo<TestEquivocationDetectionPipeline> {
	HeaderFinalityInfo::<TestEquivocationDetectionPipeline> {
		finality_proof: TestFinalityProof(source_hdr, vec![]),
		new_verification_context: check_following_equivocations.map(
			|check_following_equivocations| TestFinalityVerificationContext {
				check_equivocations: check_following_equivocations,
			},
		),
	}
}
