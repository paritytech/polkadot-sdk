// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

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

		cmd.run(config, partial.client, db, storage)
	}
}
