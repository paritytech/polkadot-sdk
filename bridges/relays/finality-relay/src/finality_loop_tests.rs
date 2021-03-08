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

use crate::finality_loop::{
	prune_recent_finality_proofs, prune_unjustified_headers, run, FinalityProofs, FinalitySyncParams, SourceClient,
	TargetClient, UnjustifiedHeaders,
};
use crate::{FinalityProof, FinalitySyncPipeline, SourceHeader};

use async_trait::async_trait;
use futures::{FutureExt, Stream, StreamExt};
use parking_lot::Mutex;
use relay_utils::{relay_loop::Client as RelayClient, MaybeConnectionError};
use std::{collections::HashMap, pin::Pin, sync::Arc, time::Duration};

type IsMandatory = bool;
type TestNumber = u64;

#[derive(Debug, Clone)]
enum TestError {
	NonConnection,
}

impl MaybeConnectionError for TestError {
	fn is_connection_error(&self) -> bool {
		false
	}
}

#[derive(Debug, Clone)]
struct TestFinalitySyncPipeline;

impl FinalitySyncPipeline for TestFinalitySyncPipeline {
	const SOURCE_NAME: &'static str = "TestSource";
	const TARGET_NAME: &'static str = "TestTarget";

	type Hash = u64;
	type Number = TestNumber;
	type Header = TestSourceHeader;
	type FinalityProof = TestFinalityProof;
}

#[derive(Debug, Clone, PartialEq)]
struct TestSourceHeader(IsMandatory, TestNumber);

impl SourceHeader<TestNumber> for TestSourceHeader {
	fn number(&self) -> TestNumber {
		self.1
	}

	fn is_mandatory(&self) -> bool {
		self.0
	}
}

#[derive(Debug, Clone, PartialEq)]
struct TestFinalityProof(Option<TestNumber>);

impl FinalityProof<TestNumber> for TestFinalityProof {
	fn target_header_number(&self) -> Option<TestNumber> {
		self.0
	}
}

#[derive(Debug, Clone, Default)]
struct ClientsData {
	source_best_block_number: TestNumber,
	source_headers: HashMap<TestNumber, (TestSourceHeader, Option<TestFinalityProof>)>,
	source_proofs: Vec<TestFinalityProof>,

	target_best_block_number: TestNumber,
	target_headers: Vec<(TestSourceHeader, TestFinalityProof)>,
}

#[derive(Clone)]
struct TestSourceClient {
	on_method_call: Arc<dyn Fn(&mut ClientsData) + Send + Sync>,
	data: Arc<Mutex<ClientsData>>,
}

#[async_trait]
impl RelayClient for TestSourceClient {
	type Error = TestError;

	async fn reconnect(&mut self) -> Result<(), TestError> {
		unreachable!()
	}
}

#[async_trait]
impl SourceClient<TestFinalitySyncPipeline> for TestSourceClient {
	type FinalityProofsStream = Pin<Box<dyn Stream<Item = TestFinalityProof>>>;

	async fn best_finalized_block_number(&self) -> Result<TestNumber, TestError> {
		let mut data = self.data.lock();
		(self.on_method_call)(&mut *data);
		Ok(data.source_best_block_number)
	}

	async fn header_and_finality_proof(
		&self,
		number: TestNumber,
	) -> Result<(TestSourceHeader, Option<TestFinalityProof>), TestError> {
		let mut data = self.data.lock();
		(self.on_method_call)(&mut *data);
		data.source_headers
			.get(&number)
			.cloned()
			.ok_or(TestError::NonConnection)
	}

	async fn finality_proofs(&self) -> Result<Self::FinalityProofsStream, TestError> {
		let mut data = self.data.lock();
		(self.on_method_call)(&mut *data);
		Ok(futures::stream::iter(data.source_proofs.clone()).boxed())
	}
}

#[derive(Clone)]
struct TestTargetClient {
	on_method_call: Arc<dyn Fn(&mut ClientsData) + Send + Sync>,
	data: Arc<Mutex<ClientsData>>,
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
	async fn best_finalized_source_block_number(&self) -> Result<TestNumber, TestError> {
		let mut data = self.data.lock();
		(self.on_method_call)(&mut *data);
		Ok(data.target_best_block_number)
	}

	async fn submit_finality_proof(&self, header: TestSourceHeader, proof: TestFinalityProof) -> Result<(), TestError> {
		let mut data = self.data.lock();
		(self.on_method_call)(&mut *data);
		data.target_best_block_number = header.number();
		data.target_headers.push((header, proof));
		Ok(())
	}
}

fn run_sync_loop(state_function: impl Fn(&mut ClientsData) -> bool + Send + Sync + 'static) -> ClientsData {
	let (exit_sender, exit_receiver) = futures::channel::mpsc::unbounded();
	let internal_state_function: Arc<dyn Fn(&mut ClientsData) + Send + Sync> = Arc::new(move |data| {
		if state_function(data) {
			exit_sender.unbounded_send(()).unwrap();
		}
	});
	let clients_data = Arc::new(Mutex::new(ClientsData {
		source_best_block_number: 10,
		source_headers: vec![
			(6, (TestSourceHeader(false, 6), None)),
			(7, (TestSourceHeader(false, 7), Some(TestFinalityProof(Some(7))))),
			(8, (TestSourceHeader(true, 8), Some(TestFinalityProof(Some(8))))),
			(9, (TestSourceHeader(false, 9), Some(TestFinalityProof(Some(9))))),
			(10, (TestSourceHeader(false, 10), None)),
		]
		.into_iter()
		.collect(),
		source_proofs: vec![TestFinalityProof(Some(12)), TestFinalityProof(Some(14))],

		target_best_block_number: 5,
		target_headers: vec![],
	}));
	let source_client = TestSourceClient {
		on_method_call: internal_state_function.clone(),
		data: clients_data.clone(),
	};
	let target_client = TestTargetClient {
		on_method_call: internal_state_function,
		data: clients_data.clone(),
	};
	let sync_params = FinalitySyncParams {
		tick: Duration::from_secs(0),
		recent_finality_proofs_limit: 1024,
		stall_timeout: Duration::from_secs(1),
	};

	run(
		source_client,
		target_client,
		sync_params,
		None,
		exit_receiver.into_future().map(|(_, _)| ()),
	);

	let clients_data = clients_data.lock().clone();
	clients_data
}

#[test]
fn finality_sync_loop_works() {
	let client_data = run_sync_loop(|data| {
		// header#7 has persistent finality proof, but it isn't mandatory => it isn't submitted, because
		// header#8 has persistent finality proof && it is mandatory => it is submitted
		// header#9 has persistent finality proof, but it isn't mandatory => it is submitted, because
		//   there are no more persistent finality proofs
		//
		// once this ^^^ is done, we generate more blocks && read proof for blocks 12, 14 and 16 from the stream
		// but we only submit proof for 16
		//
		// proof for block 15 is ignored - we haven't managed to decode it
		if data.target_best_block_number == 9 {
			data.source_best_block_number = 17;
			data.source_headers.insert(11, (TestSourceHeader(false, 11), None));
			data.source_headers
				.insert(12, (TestSourceHeader(false, 12), Some(TestFinalityProof(Some(12)))));
			data.source_headers.insert(13, (TestSourceHeader(false, 13), None));
			data.source_headers
				.insert(14, (TestSourceHeader(false, 14), Some(TestFinalityProof(Some(14)))));
			data.source_headers
				.insert(15, (TestSourceHeader(false, 15), Some(TestFinalityProof(None))));
			data.source_headers
				.insert(16, (TestSourceHeader(false, 16), Some(TestFinalityProof(Some(16)))));
			data.source_headers.insert(17, (TestSourceHeader(false, 17), None));
		}

		data.target_best_block_number == 16
	});

	assert_eq!(
		client_data.target_headers,
		vec![
			(TestSourceHeader(true, 8), TestFinalityProof(Some(8))),
			(TestSourceHeader(false, 9), TestFinalityProof(Some(9))),
			(TestSourceHeader(false, 16), TestFinalityProof(Some(16))),
		],
	);
}

#[test]
fn prune_unjustified_headers_works() {
	let original_unjustified_headers: UnjustifiedHeaders<TestFinalitySyncPipeline> = vec![
		TestSourceHeader(false, 10),
		TestSourceHeader(false, 13),
		TestSourceHeader(false, 15),
		TestSourceHeader(false, 17),
		TestSourceHeader(false, 19),
	]
	.into_iter()
	.collect();

	// when header is in the collection
	let mut unjustified_headers = original_unjustified_headers.clone();
	assert_eq!(
		prune_unjustified_headers::<TestFinalitySyncPipeline>(10, &mut unjustified_headers),
		Some(TestSourceHeader(false, 10)),
	);
	assert_eq!(&original_unjustified_headers[1..], unjustified_headers,);

	// when the header doesn't exist in the collection
	let mut unjustified_headers = original_unjustified_headers.clone();
	assert_eq!(
		prune_unjustified_headers::<TestFinalitySyncPipeline>(11, &mut unjustified_headers),
		None,
	);
	assert_eq!(&original_unjustified_headers[1..], unjustified_headers,);

	// when last entry is pruned
	let mut unjustified_headers = original_unjustified_headers.clone();
	assert_eq!(
		prune_unjustified_headers::<TestFinalitySyncPipeline>(19, &mut unjustified_headers),
		Some(TestSourceHeader(false, 19)),
	);

	assert_eq!(&original_unjustified_headers[5..], unjustified_headers,);

	// when we try and prune past last entry
	let mut unjustified_headers = original_unjustified_headers.clone();
	assert_eq!(
		prune_unjustified_headers::<TestFinalitySyncPipeline>(20, &mut unjustified_headers),
		None,
	);
	assert_eq!(&original_unjustified_headers[5..], unjustified_headers,);
}

#[test]
fn prune_recent_finality_proofs_works() {
	let original_recent_finality_proofs: FinalityProofs<TestFinalitySyncPipeline> = vec![
		(10, TestFinalityProof(Some(10))),
		(13, TestFinalityProof(Some(13))),
		(15, TestFinalityProof(Some(15))),
		(17, TestFinalityProof(Some(17))),
		(19, TestFinalityProof(Some(19))),
	]
	.into_iter()
	.collect();

	// when there's proof for justified header in the vec
	let mut recent_finality_proofs = original_recent_finality_proofs.clone();
	prune_recent_finality_proofs::<TestFinalitySyncPipeline>(10, &mut recent_finality_proofs, 1024);
	assert_eq!(&original_recent_finality_proofs[1..], recent_finality_proofs,);

	// when there are no proof for justified header in the vec
	let mut recent_finality_proofs = original_recent_finality_proofs.clone();
	prune_recent_finality_proofs::<TestFinalitySyncPipeline>(11, &mut recent_finality_proofs, 1024);
	assert_eq!(&original_recent_finality_proofs[1..], recent_finality_proofs,);

	// when there are too many entries after initial prune && they also need to be pruned
	let mut recent_finality_proofs = original_recent_finality_proofs.clone();
	prune_recent_finality_proofs::<TestFinalitySyncPipeline>(10, &mut recent_finality_proofs, 2);
	assert_eq!(&original_recent_finality_proofs[3..], recent_finality_proofs,);

	// when last entry is pruned
	let mut recent_finality_proofs = original_recent_finality_proofs.clone();
	prune_recent_finality_proofs::<TestFinalitySyncPipeline>(19, &mut recent_finality_proofs, 2);
	assert_eq!(&original_recent_finality_proofs[5..], recent_finality_proofs,);

	// when post-last entry is pruned
	let mut recent_finality_proofs = original_recent_finality_proofs.clone();
	prune_recent_finality_proofs::<TestFinalitySyncPipeline>(20, &mut recent_finality_proofs, 2);
	assert_eq!(&original_recent_finality_proofs[5..], recent_finality_proofs,);
}
