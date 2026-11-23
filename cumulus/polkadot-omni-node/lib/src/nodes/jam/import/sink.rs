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

//! Feeds blocks recovered from JAM's DA layer into the import queue.

use cumulus_client_jam_work_package_recovery::{ImportBlocksSink, RecoveredBlock};
use sc_consensus::{import_queue::ImportQueueService, IncomingBlock};
use sp_consensus::BlockOrigin;
use sp_runtime::traits::Block as BlockT;

pub(crate) struct JamImportSink<Block: BlockT> {
	pub(crate) service: Box<dyn ImportQueueService<Block>>,
}

impl<Block: BlockT> ImportBlocksSink<Block> for JamImportSink<Block> {
	fn import_blocks(&mut self, blocks: Vec<RecoveredBlock<Block>>) {
		let incoming: Vec<IncomingBlock<Block>> = blocks
			.into_iter()
			.map(|b| IncomingBlock {
				hash: b.hash,
				header: Some(b.header),
				body: Some(b.body),
				indexed_body: None,
				justifications: None,
				origin: None,
				allow_missing_state: true,
				import_existing: false,
				state: None,
				skip_execution: false,
				additional_data: b.additional_data,
			})
			.collect();
		self.service.import_blocks(BlockOrigin::NetworkBroadcast, incoming);
	}
}
