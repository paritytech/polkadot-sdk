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

//! Command execution utilities for parachain nodes.
//!
//! This module provides traits and implementations for handling various
//! CLI commands that can be executed by a parachain node.

use crate::common::spec::BaseNodeSpec;
use cumulus_client_cli::ExportGenesisHeadCommand;
use frame_benchmarking_cli::BlockCmd;
#[cfg(any(feature = "runtime-benchmarks"))]
use frame_benchmarking_cli::StorageCmd;
use sc_cli::{CheckBlockCmd, ExportBlocksCmd, ExportStateCmd, ImportBlocksCmd, RevertCmd};
use sc_service::{Configuration, TaskManager};
use std::{future::Future, pin::Pin};

/// Result type for synchronous CLI commands.
type SyncCmdResult = sc_cli::Result<()>;

/// Result type for asynchronous CLI commands.
///
/// Returns a tuple containing:
/// * A pinned boxed future that resolves to the command result
/// * A TaskManager for managing the command's async tasks
type AsyncCmdResult<'a> =
	sc_cli::Result<(Pin<Box<dyn Future<Output = SyncCmdResult> + 'a>>, TaskManager)>;

/// A trait for executing various CLI commands in a parachain node.
///
/// Implementors of this trait provide the logic for preparing and running
/// different types of commands such as checking blocks, exporting data,
/// and running benchmarks.
pub trait NodeCommandRunner {
	/// Prepares the command for checking a specific block.
	///
	/// # Arguments
	/// * `self` - The command runner instance (consumed)
	/// * `config` - The node configuration
	/// * `cmd` - The check block command parameters
	///
	/// # Returns
	/// An asynchronous command result that will execute the block check
	fn prepare_check_block_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &CheckBlockCmd,
	) -> AsyncCmdResult<'_>;

	/// Prepares the command for exporting blocks from the database.
	///
	/// # Arguments
	/// * `self` - The command runner instance (consumed)
	/// * `config` - The node configuration
	/// * `cmd` - The export blocks command parameters
	///
	/// # Returns
	/// An asynchronous command result that will execute the block export
	fn prepare_export_blocks_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &ExportBlocksCmd,
	) -> AsyncCmdResult<'_>;

	/// Prepares the command for exporting state from a specific block.
	///
	/// # Arguments
	/// * `self` - The command runner instance (consumed)
	/// * `config` - The node configuration
	/// * `cmd` - The export state command parameters
	///
	/// # Returns
	/// An asynchronous command result that will execute the state export
	fn prepare_export_state_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &ExportStateCmd,
	) -> AsyncCmdResult<'_>;

	/// Prepares the command for importing blocks into the database.
	///
	/// # Arguments
	/// * `self` - The command runner instance (consumed)
	/// * `config` - The node configuration
	/// * `cmd` - The import blocks command parameters
	///
	/// # Returns
	/// An asynchronous command result that will execute the block import
	fn prepare_import_blocks_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &ImportBlocksCmd,
	) -> AsyncCmdResult<'_>;

	/// Prepares the command for reverting blocks from the database.
	///
	/// # Arguments
	/// * `self` - The command runner instance (consumed)
	/// * `config` - The node configuration
	/// * `cmd` - The revert command parameters
	///
	/// # Returns
	/// An asynchronous command result that will execute the revert operation
	fn prepare_revert_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &RevertCmd,
	) -> AsyncCmdResult<'_>;

	/// Runs the command to export the genesis head of the parachain.
	///
	/// # Arguments
	/// * `self` - The command runner instance (consumed)
	/// * `config` - The node configuration
	/// * `cmd` - The export genesis head command parameters
	///
	/// # Returns
	/// A synchronous command result indicating success or failure
	fn run_export_genesis_head_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &ExportGenesisHeadCommand,
	) -> SyncCmdResult;

	/// Runs the block benchmarking command.
	///
	/// # Arguments
	/// * `self` - The command runner instance (consumed)
	/// * `config` - The node configuration
	/// * `cmd` - The block benchmark command parameters
	///
	/// # Returns
	/// A synchronous command result indicating success or failure
	fn run_benchmark_block_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &BlockCmd,
	) -> SyncCmdResult;

	/// Runs the storage benchmarking command.
	///
	/// This is only available when the `runtime-benchmarks` feature is enabled.
	///
	/// # Arguments
	/// * `self` - The command runner instance (consumed)
	/// * `config` - The node configuration
	/// * `cmd` - The storage benchmark command parameters
	///
	/// # Returns
	/// A synchronous command result indicating success or failure
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
