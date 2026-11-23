// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! What the builder task and the collation task share: the message for one built block, and
//! the authoring hold.

use jam_interface::{BlockDesc, CoreIndex, Slot as JamSlot};
use jam_types::RefineContext;
use sp_additional_data::AdditionalData;
use sp_runtime::traits::Block as BlockT;
use std::sync::{
	atomic::{AtomicUsize, Ordering},
	Arc,
};

/// How many in-flight resubmission packages the builder has to wait for before it authors again.
/// Written by the collation task, read by the builder at tick time.
#[derive(Clone)]
pub(crate) struct AuthoringHold(Arc<AtomicUsize>);

impl AuthoringHold {
	pub(crate) fn new() -> Self {
		Self(Arc::new(AtomicUsize::new(0)))
	}

	pub(crate) fn set_pending(&self, n: usize) {
		self.0.store(n, Ordering::Relaxed);
	}

	pub(crate) fn pending(&self) -> usize {
		self.0.load(Ordering::Relaxed)
	}
}

/// Message from the builder task to the collation task: one built parachain block plus the JAM
/// context it was built against.
pub(crate) struct JamCollatorMessage<Block: BlockT> {
	pub parent_header: Block::Header,
	pub block: Block,
	pub proof: sp_api::StorageProof,
	/// The refine context captured at build time; the anchor inside it decides the submission
	/// window.
	pub context: RefineContext,
	/// The timeslot of the anchor block, which starts the ~8-block clock the package has to be
	/// reported within. The collation task needs it to tell an expired anchor apart from any
	/// other reason a package failed.
	pub anchor_slot: JamSlot,
	/// The core whose authorizer pool held this para's authorizer at the anchor, if any. `None`
	/// means the package must not be submitted anywhere — no guarantor would authorize it.
	pub submit_target: Option<CoreIndex>,
	/// The additional-data map to carry in the PoV: the `JAM_PROOF_KEY` entry holding the
	/// SCALE-encoding of `(anchor_state_root, lookup_state_root, proof)` for the para head at
	/// the anchor.
	pub additional_data: AdditionalData,
	/// The JAM best block that triggered this build (for logging).
	pub triggered_by: BlockDesc,
}
