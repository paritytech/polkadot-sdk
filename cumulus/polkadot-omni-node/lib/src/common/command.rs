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

use crate::common::spec::BaseNodeSpec;
use cumulus_client_cli::ExportGenesisHeadCommand;
use frame_benchmarking_cli::BlockCmd;
#[cfg(any(feature = "runtime-benchmarks"))]
use frame_benchmarking_cli::StorageCmd;
use sc_cli::{CheckBlockCmd, ExportBlocksCmd, ExportStateCmd, ImportBlocksCmd, RevertCmd};
use sc_service::{Configuration, TaskManager};
use std::{future::Future, pin::Pin};

type SyncCmdResult = sc_cli::Result<()>;

type AsyncCmdResult<'a> =
	sc_cli::Result<(Pin<Box<dyn Future<Output = SyncCmdResult> + 'a>>, TaskManager)>;

pub trait NodeCommandRunner {
	fn prepare_check_block_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &CheckBlockCmd,
	) -> AsyncCmdResult<'_>;

	fn prepare_export_blocks_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &ExportBlocksCmd,
	) -> AsyncCmdResult<'_>;

	fn prepare_export_state_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &ExportStateCmd,
	) -> AsyncCmdResult<'_>;

	fn prepare_import_blocks_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &ImportBlocksCmd,
	) -> AsyncCmdResult<'_>;

	fn prepare_revert_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &RevertCmd,
	) -> AsyncCmdResult<'_>;

	fn run_export_genesis_head_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &ExportGenesisHeadCommand,
	) -> SyncCmdResult;

	fn run_benchmark_block_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &BlockCmd,
	) -> SyncCmdResult;

	#[cfg(any(feature = "runtime-benchmarks"))]
	fn run_benchmark_storage_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &StorageCmd,
	) -> SyncCmdResult;
}

impl<T> NodeCommandRunner for T
where
	T: BaseNodeSpec,
{
	fn prepare_check_block_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &CheckBlockCmd,
	) -> AsyncCmdResult<'_> {
		let partial = T::new_partial(&config).map_err(sc_cli::Error::Service)?;
		Ok((Box::pin(cmd.run(partial.client, partial.import_queue)), partial.task_manager))
	}

	fn prepare_export_blocks_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &ExportBlocksCmd,
	) -> AsyncCmdResult<'_> {
		let partial = T::new_partial(&config).map_err(sc_cli::Error::Service)?;
		Ok((Box::pin(cmd.run(partial.client, config.database)), partial.task_manager))
	}

	fn prepare_export_state_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &ExportStateCmd,
	) -> AsyncCmdResult<'_> {
		let partial = T::new_partial(&config).map_err(sc_cli::Error::Service)?;
		Ok((Box::pin(cmd.run(partial.client, config.chain_spec)), partial.task_manager))
	}

	fn prepare_import_blocks_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &ImportBlocksCmd,
	) -> AsyncCmdResult<'_> {
		let partial = T::new_partial(&config).map_err(sc_cli::Error::Service)?;
		Ok((Box::pin(cmd.run(partial.client, partial.import_queue)), partial.task_manager))
	}

	fn prepare_revert_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &RevertCmd,
	) -> AsyncCmdResult<'_> {
		let partial = T::new_partial(&config).map_err(sc_cli::Error::Service)?;
		Ok((Box::pin(cmd.run(partial.client, partial.backend, None)), partial.task_manager))
	}

	fn run_export_genesis_head_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &ExportGenesisHeadCommand,
	) -> SyncCmdResult {
		let partial = T::new_partial(&config).map_err(sc_cli::Error::Service)?;
		cmd.run(partial.client)
	}

	fn run_benchmark_block_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &BlockCmd,
	) -> SyncCmdResult {
		let partial = T::new_partial(&config).map_err(sc_cli::Error::Service)?;
		cmd.run(partial.client)
	}

	#[cfg(any(feature = "runtime-benchmarks"))]
	fn run_benchmark_storage_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &StorageCmd,
	) -> SyncCmdResult {
		let partial = T::new_partial(&config).map_err(sc_cli::Error::Service)?;
		let db = partial.backend.expose_db();
		let storage = partial.backend.expose_storage();
		let shared_trie_cache = partial.backend.expose_shared_trie_cache();

		cmd.run(config, partial.client, db, storage, shared_trie_cache)
	}
}
