// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Defines the PVQ executor.
use alloc::vec::Vec;
use polkavm::{Config, Engine, Linker, Module, ModuleConfig, ProgramBlob};

use crate::{context::PvqExecutorContext, error::PvqExecutorError};

/// The result of a PVQ execution.
type PvqExecutorResult<UserError> = Result<Vec<u8>, PvqExecutorError<UserError>>;
/// The gas limit for a PVQ execution.
type GasLimit = Option<i64>;

/// Executes a PVQ guest program inside a [`polkavm`] instance.
///
/// The executor is parameterized by a [`PvqExecutorContext`], which registers host functions and
/// provides the mutable user data passed to host calls.
pub struct PvqExecutor<Ctx: PvqExecutorContext> {
	engine: Engine,
	linker: Linker<Ctx::UserData, Ctx::UserError>,
	context: Ctx,
}

impl<Ctx: PvqExecutorContext> PvqExecutor<Ctx> {
	/// Creates a new [`PvqExecutor`].
	///
	/// # Arguments
	///
	/// - `config`: The PolkaVM configuration used to create the underlying [`Engine`].
	/// - `context`: The executor context used to register host functions and provide user data.
	///
	/// # Panics
	///
	/// Panics if the [`Engine`] cannot be created from `config`.
	pub fn new(config: Config, mut context: Ctx) -> Self {
		let engine = Engine::new(&config)
			.expect("PolkaVM engine creation must succeed for a valid configuration");
		let mut linker = Linker::<Ctx::UserData, Ctx::UserError>::new();
		// Register user-defined host functions
		context.register_host_functions(&mut linker);
		Self { engine, linker, context }
	}

	/// Executes a PVQ guest program.
	///
	/// # Arguments
	///
	/// - `program`: A PolkaVM program blob.
	/// - `args`: Opaque argument bytes that are written into the module's auxiliary data region.
	/// - `gas_limit`: If `Some`, enables PolkaVM gas metering and sets the initial gas.
	///
	/// # Returns
	///
	/// Returns `(result, gas_remaining)`.
	///
	/// - If `gas_limit` is `Some`, `gas_remaining` is `Some(remaining_gas)`.
	/// - If `gas_limit` is `None`, gas metering is disabled and `gas_remaining` is `None`.
	///
	/// The guest is expected to export an entrypoint called `"pvq"`. The executor passes the
	/// auxiliary data pointer and the argument length as the entrypoint parameters.
	pub fn execute(
		&mut self,
		program: &[u8],
		args: &[u8],
		gas_limit: GasLimit,
	) -> (PvqExecutorResult<Ctx::UserError>, GasLimit) {
		let blob = match ProgramBlob::parse(program.into()) {
			Ok(blob) => blob,
			Err(_) => return (Err(PvqExecutorError::InvalidProgramFormat), gas_limit),
		};

		let mut module_config = ModuleConfig::new();
		module_config.set_aux_data_size(args.len() as u32);
		if gas_limit.is_some() {
			module_config.set_gas_metering(Some(polkavm::GasMeteringKind::Sync));
		}

		let module = match Module::from_blob(&self.engine, &module_config, blob) {
			Ok(module) => module,
			Err(err) => return (Err(err.into()), gas_limit),
		};

		let instance_pre = match self.linker.instantiate_pre(&module) {
			Ok(instance_pre) => instance_pre,
			Err(err) => return (Err(err.into()), gas_limit),
		};

		let mut instance = match instance_pre.instantiate() {
			Ok(instance) => instance,
			Err(err) => return (Err(err.into()), gas_limit),
		};

		if let Some(gas_limit) = gas_limit {
			instance.set_gas(gas_limit);
		}

		// From this point on, we include instance.gas() in the return value
		let result = (|| {
			instance.write_memory(module.memory_map().aux_data_address(), args)?;

			tracing::info!("Calling entrypoint with args: {:?}", args);
			let res = instance.call_typed_and_get_result::<u64, (u32, u32)>(
				self.context.data(),
				"pvq",
				(module.memory_map().aux_data_address(), args.len() as u32),
			)?;

			let res_size = (res >> 32) as u32;
			let res_ptr = (res & 0xffffffff) as u32;

			let result = instance.read_memory(res_ptr, res_size)?;

			Ok(result)
		})();

		if gas_limit.is_some() {
			(result, Some(instance.gas()))
		} else {
			(result, None)
		}
	}
}
