// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! A generic statement distribution subsystem mockup suitable to be used in benchmarks.

use crate::statement::TestState;
use futures::FutureExt;
use polkadot_node_network_protocol::{
	request_response::{
		v2::{AttestedCandidateRequest, AttestedCandidateResponse},
		IncomingRequestReceiver,
	},
	UnifiedReputationChange,
};
use polkadot_node_subsystem::{
	overseer, FromOrchestra, OverseerSignal, SpawnedSubsystem, SubsystemError,
};
use std::sync::Arc;

const COST_INVALID_REQUEST: UnifiedReputationChange =
	UnifiedReputationChange::CostMajor("Peer sent unparsable request");

pub struct MockStatementDistribution {
	/// Receiver for attested candidate requests.
	req_receiver: IncomingRequestReceiver<AttestedCandidateRequest>,
	test_state: Arc<TestState>,
}

impl MockStatementDistribution {
	pub fn new(
		req_receiver: IncomingRequestReceiver<AttestedCandidateRequest>,
		test_state: Arc<TestState>,
	) -> Self {
		Self { req_receiver, test_state }
	}
}

#[overseer::subsystem(StatementDistribution, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockStatementDistribution {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();
		SpawnedSubsystem { name: "test-environment", future }
	}
}

#[overseer::contextbounds(StatementDistribution, prefix = self::overseer)]
impl MockStatementDistribution {
	async fn run<Context>(mut self, mut ctx: Context) {
		loop {
			tokio::select! {
				msg = ctx.recv() => match msg {
					Ok(FromOrchestra::Signal(OverseerSignal::Conclude)) => return,
					Ok(FromOrchestra::Communication { msg }) =>
						println!("🚨🚨🚨 Received message: {:?}", msg),
					err => println!("🚨🚨🚨 recv error: {:?}", err),
				},
				req = self.req_receiver.recv(|| vec![COST_INVALID_REQUEST]) => {
					let req = req.expect("Receiver never fails");
					let candidate_receipt = self
						.test_state
						.commited_candidate_receipts
						.values()
						.flatten()
						.find(|v| v.hash() == req.payload.candidate_hash)
						.unwrap()
						.clone();
					let persisted_validation_data = self.test_state.pvd.clone();
					let statements = self.test_state.statements.get(&req.payload.candidate_hash).unwrap().clone();
					let res = AttestedCandidateResponse {
						candidate_receipt,
						persisted_validation_data,
						statements,
					};
					let _ = req.send_response(res);
					println!("❤️❤️❤️ Sent candidate response");
				}
			}
		}
	}
}
