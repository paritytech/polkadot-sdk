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

//! Helper trait used to dynamically run parachain node commands.

use cumulus_client_cli::ExportGenesisHeadCommand;
use cumulus_primitives_core::BlockT;
use frame_benchmarking_cli::BlockCmd;
#[cfg(any(feature = "runtime-benchmarks"))]
use frame_benchmarking_cli::StorageCmd;
use sc_block_builder::BlockBuilderApi;
use sc_cli::{CheckBlockCmd, ExportBlocksCmd, ExportStateCmd, ImportBlocksCmd, RevertCmd};
use sc_client_api::{BlockBackend, StorageProvider, UsageProvider};
use sc_client_db::{Backend, DbHash};
use sc_service::{Configuration, PartialComponents, TaskManager};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::{traits::Header as HeaderT, OpaqueExtrinsic};
use std::{fmt::Debug, future::Future, pin::Pin, str::FromStr};

type SyncCmdResult = sc_cli::Result<()>;
type AsyncCmdResult<'a> = (Pin<Box<dyn Future<Output = SyncCmdResult> + 'a>>, TaskManager);

pub trait CmdRunner<Block: BlockT> {
	fn prepare_check_block_cmd(self: Box<Self>, cmd: &CheckBlockCmd) -> AsyncCmdResult<'_>;

	fn prepare_export_blocks_cmd(
		self: Box<Self>,
		cmd: &ExportBlocksCmd,
		config: Configuration,
	) -> AsyncCmdResult<'_>;

	fn prepare_export_state_cmd(
		self: Box<Self>,
		cmd: &ExportStateCmd,
		config: Configuration,
	) -> AsyncCmdResult<'_>;

	fn prepare_import_blocks_cmd(self: Box<Self>, cmd: &ImportBlocksCmd) -> AsyncCmdResult<'_>;

	fn prepare_revert_cmd(self: Box<Self>, cmd: &RevertCmd) -> AsyncCmdResult<'_>;

	fn run_export_genesis_head_cmd(
		self: Box<Self>,
		cmd: &ExportGenesisHeadCommand,
	) -> SyncCmdResult;

	fn run_benchmark_block_cmd(self: Box<Self>, cmd: &BlockCmd) -> SyncCmdResult;

	#[cfg(any(feature = "runtime-benchmarks"))]
	fn run_benchmark_storage_cmd(
		self: Box<Self>,
		cmd: &StorageCmd,
		config: Configuration,
	) -> SyncCmdResult;
}

impl<Block, Client, SelectChain, ImportQueue, TransactionPool, Other> CmdRunner<Block>
	for PartialComponents<Client, Backend<Block>, SelectChain, ImportQueue, TransactionPool, Other>
where
	Self: Send,
	Block: BlockT<Extrinsic = OpaqueExtrinsic, Hash = DbHash> + for<'de> serde::Deserialize<'de>,
	<<Block::Header as HeaderT>::Number as FromStr>::Err: Debug,
	Client: HeaderBackend<Block>
		+ BlockBackend<Block>
		+ ProvideRuntimeApi<Block>
		+ StorageProvider<Block, Backend<Block>>
		+ UsageProvider<Block>
		+ 'static,
	Client::Api: BlockBuilderApi<Block>,
	ImportQueue: sc_service::ImportQueue<Block> + 'static,
{
	fn prepare_check_block_cmd(self: Box<Self>, cmd: &CheckBlockCmd) -> AsyncCmdResult<'_> {
		(Box::pin(cmd.run(self.client, self.import_queue)), self.task_manager)
	}

	fn prepare_export_blocks_cmd(
		self: Box<Self>,
		cmd: &ExportBlocksCmd,
		config: Configuration,
	) -> AsyncCmdResult<'_> {
		(Box::pin(cmd.run(self.client, config.database)), self.task_manager)
	}

	fn prepare_export_state_cmd(
		self: Box<Self>,
		cmd: &ExportStateCmd,
		config: Configuration,
	) -> AsyncCmdResult<'_> {
		(Box::pin(cmd.run(self.client, config.chain_spec)), self.task_manager)
	}

	fn prepare_import_blocks_cmd(self: Box<Self>, cmd: &ImportBlocksCmd) -> AsyncCmdResult<'_> {
		(Box::pin(cmd.run(self.client, self.import_queue)), self.task_manager)
	}

	fn prepare_revert_cmd(self: Box<Self>, cmd: &RevertCmd) -> AsyncCmdResult<'_> {
		(Box::pin(cmd.run(self.client, self.backend, None)), self.task_manager)
	}

	fn run_export_genesis_head_cmd(
		self: Box<Self>,
		cmd: &ExportGenesisHeadCommand,
	) -> SyncCmdResult {
		cmd.run(self.client)
	}

	fn run_benchmark_block_cmd(self: Box<Self>, cmd: &BlockCmd) -> SyncCmdResult {
		cmd.run(self.client)
	}

	#[cfg(any(feature = "runtime-benchmarks"))]
	fn run_benchmark_storage_cmd(
		self: Box<Self>,
		cmd: &StorageCmd,
		config: Configuration,
	) -> SyncCmdResult {
		let db = self.backend.expose_db();
		let storage = self.backend.expose_storage();

		cmd.run(config, self.client, db, storage)
	}
}
