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

use super::ApprovalTestState;
use futures::FutureExt;
use polkadot_node_subsystem::{overseer, SpawnedSubsystem, SubsystemError};
use polkadot_node_subsystem_types::messages::{RuntimeApiMessage, RuntimeApiRequest};
use polkadot_primitives::{vstaging::NodeFeatures, ExecutorParams};

/// Mock RuntimeApi subsystem used to answer request made by the approval-voting subsystem,
/// during benchmark. All the necessary information to answer the requests is stored in the `state`
pub struct MockRuntimeApi {
	pub state: ApprovalTestState,
}
#[overseer::subsystem(RuntimeApi, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockRuntimeApi {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "runtime-api-subsystem", future }
	}
}

#[overseer::contextbounds(RuntimeApi, prefix = self::overseer)]
impl MockRuntimeApi {
	async fn run<Context>(self, mut ctx: Context) {
		loop {
			let msg = ctx.recv().await.expect("Should not fail");

			match msg {
				orchestra::FromOrchestra::Signal(_) => {},
				orchestra::FromOrchestra::Communication { msg } => match msg {
					RuntimeApiMessage::Request(
						request,
						RuntimeApiRequest::CandidateEvents(sender),
					) => {
						let candidate_events =
							self.state.get_info_by_hash(request).candidates.clone();
						let _ = sender.send(Ok(candidate_events));
					},
					RuntimeApiMessage::Request(
						_request,
						RuntimeApiRequest::SessionIndexForChild(sender),
					) => {
						let _ = sender.send(Ok(1));
					},
					RuntimeApiMessage::Request(
						_request,
						RuntimeApiRequest::SessionInfo(_session_index, sender),
					) => {
						let _ = sender.send(Ok(Some(self.state.session_info.clone())));
					},
					RuntimeApiMessage::Request(
						_request,
						RuntimeApiRequest::NodeFeatures(_session_index, sender),
					) => {
						let _ = sender.send(Ok(NodeFeatures::EMPTY));
					},
					RuntimeApiMessage::Request(
						_request,
						RuntimeApiRequest::SessionExecutorParams(_session_index, sender),
					) => {
						let _ = sender.send(Ok(Some(ExecutorParams::default())));
					},
					RuntimeApiMessage::Request(
						_request,
						RuntimeApiRequest::CurrentBabeEpoch(sender),
					) => {
						let _ = sender.send(Ok(self.state.babe_epoch.clone()));
					},
					_ => {},
				},
			}
		}
	}
}
