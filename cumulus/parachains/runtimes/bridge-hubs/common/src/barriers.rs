// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use core::{marker::PhantomData, ops::ControlFlow};
use cumulus_primitives_core::Weight;
use frame_support::traits::{Contains, ProcessMessageError};
use xcm::prelude::{ExportMessage, Instruction, Location, NetworkId, UnpaidExecution};

use frame_support::ensure;
use sp_core::Get;
use xcm_builder::{CreateMatcher, MatchXcm};
use xcm_executor::traits::{DenyExecution, Properties, ShouldExecute};

/// Deny execution if the message contains instruction `ExportMessage` with
/// a. origin is contained in `FromOrigin` (i.e.`FromOrigin::Contains(origin)`)
/// b. network is contained in `ToGlobalConsensus`, (i.e. `ToGlobalConsensus::contains(network)`)
pub struct DenyExportMessageFrom<FromOrigin, ToGlobalConsensus>(
	PhantomData<(FromOrigin, ToGlobalConsensus)>,
);

impl<FromOrigin, ToGlobalConsensus> DenyExecution
	for DenyExportMessageFrom<FromOrigin, ToGlobalConsensus>
where
	FromOrigin: Contains<Location>,
	ToGlobalConsensus: Contains<NetworkId>,
{
	fn deny_execution<RuntimeCall>(
		origin: &Location,
		message: &mut [Instruction<RuntimeCall>],
		_max_weight: Weight,
		_properties: &mut Properties,
	) -> Result<(), ProcessMessageError> {
		// This barrier only cares about messages with `origin` matching `FromOrigin`.
		if !FromOrigin::contains(origin) {
			return Ok(())
		}
		message.matcher().match_next_inst_while(
			|_| true,
			|inst| match inst {
				ExportMessage { network, .. } if ToGlobalConsensus::contains(network) =>
					Err(ProcessMessageError::Unsupported),
				_ => Ok(ControlFlow::Continue(())),
			},
		)?;
		Ok(())
	}
}

/// Allow unpaid execution from location L when exporting to network N
pub struct AllowExplicitUnpaidExecutionFromAssetHubExportToEthereum<L, N>(PhantomData<(L, N)>);
impl<L: Contains<Location>, N: Get<NetworkId>> ShouldExecute
	for AllowExplicitUnpaidExecutionFromAssetHubExportToEthereum<L, N>
{
	fn should_execute<Call>(
		origin: &Location,
		instructions: &mut [Instruction<Call>],
		max_weight: Weight,
		properties: &mut Properties,
	) -> Result<(), ProcessMessageError> {
		tracing::trace!(
			target: "xcm::barriers",
			?origin, ?instructions, ?max_weight, ?properties,
			"AllowExplicitUnpaidExecutionFromAssetHubExportToEthereum",
		);
		ensure!(L::contains(origin), ProcessMessageError::Unsupported);
		instructions
			.matcher()
			.match_next_inst(|inst| match inst {
				UnpaidExecution { .. } => Ok(()),
				_ => Err(ProcessMessageError::BadFormat),
			})?
			.match_next_inst(|inst| match inst {
				ExportMessage { network, .. } if network.clone().eq(&N::get()) => Ok(()),
				_ => Err(ProcessMessageError::BadFormat),
			})?;

		Ok(())
	}
}
