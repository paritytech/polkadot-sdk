// This file is part of Substrate.

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

//! All JIT-specific contract execution machinery.
//!
//! Declared as a submodule of [`super`] (the polkavm host integration) under
//! `cfg(any(revive_jit, feature = "runtime-benchmarks"))`, so every item
//! defined here is automatically gated and has private-field access to
//! `PreparedCall`. The few hand-off points outside this module that decide
//! the JIT backend (the `from_storage` JIT short-circuit and the `is_jit()`
//! dispatch in `ContractBlob::execute`) still need their own cfg attribute.

use super::{Interrupt, Memory, MeterBackend, PolkaVmInstance, PreparedCall, Runtime};
use crate::{
	Config, Error, LOG_TARGET,
	exec::{ExecError, Executable, Ext},
	metering::Token,
	pristine_code,
	vm::{
		CodeInfo, ContractBlob, ExportedFunction, PolkaVmWeightBackend,
		runtime_costs::WeightBackend,
	},
	weights::WeightInfo,
};
use alloc::{vec, vec::Vec};
use core::mem;
#[cfg(feature = "runtime-benchmarks")]
use environmental::environmental;
use frame_support::weights::Weight;
use sp_runtime::DispatchError;
use sp_virtualization::{
	ExecError as VirtExecError, ExecResult as VirtExecResult, Execution as VirtExecution,
	Instance as VirtInstance, Module, ModuleError,
};

#[cfg(feature = "runtime-benchmarks")]
environmental!(jit_override: bool);

/// Run `f` with the PVM execution backend forced to either JIT (`enabled = true`) or
/// interpreter (`enabled = false`), overriding the compile-time `cfg!(revive_jit)` default
/// for the duration of the closure.
///
/// Benchmark-only — production execute paths consult `cfg!(revive_jit)` directly when no
/// override is set, and this helper is unreachable outside of the `runtime-benchmarks`
/// feature.
#[cfg(feature = "runtime-benchmarks")]
pub fn with_jit_override<R>(enabled: bool, f: impl FnOnce() -> R) -> R {
	let mut enabled = enabled;
	jit_override::using_once(&mut enabled, f)
}

/// Whether the JIT backend should be selected for PVM execution in this scope.
///
/// Returns `cfg!(revive_jit)` by default; under `feature = "runtime-benchmarks"`
/// a [`with_jit_override`] scope can flip the answer for benchmark setup.
pub fn is_jit() -> bool {
	#[cfg(feature = "runtime-benchmarks")]
	{
		jit_override::with(|v| *v).unwrap_or(cfg!(revive_jit))
	}
	#[cfg(not(feature = "runtime-benchmarks"))]
	{
		cfg!(revive_jit)
	}
}

/// Marker for the [`sp_virtualization`]-backed JIT backend.
///
/// The JIT ratio is a hardcoded scaling of the interpreter ratio — by design
/// JIT per-instruction cost is not benchmarked.
pub struct JitBackend;

impl<T: Config> WeightBackend<T> for JitBackend {
	fn call_base_weight() -> Weight {
		T::WeightInfo::seal_call_jit()
	}

	fn delegate_call_base_weight() -> Weight {
		T::WeightInfo::seal_delegate_call_jit()
	}

	fn host_fn_weight() -> Weight {
		T::WeightInfo::noop_host_fn_jit(1).saturating_sub(T::WeightInfo::noop_host_fn_jit(0))
	}
}

impl<T: Config> PolkaVmWeightBackend<T> for JitBackend {
	fn ref_time_per_fuel() -> u64 {
		/// Ref time is in picoseconds.
		const REF_TIME_PER_SECOND: u64 = 1_000_000_000_000;
		/// Reference hardware is clocked at 2.6GHz.
		const CYCLES_PER_SECOND: u64 = 2_600_000_000;

		REF_TIME_PER_SECOND / CYCLES_PER_SECOND
	}
}

/// Compile-time cost of loading + JIT-compiling a contract.
///
/// `Cold` is charged on a [`sp_virtualization::Module::lookup`] miss and prices the in-runtime
/// `PristineCode` read (per-byte ref_time and proof) plus the
/// [`sp_virtualization::Module::from_bytes`] compile; `Warm` is charged on a hit and prices
/// only the lookup + instantiation of the already-compiled module — no storage read happens.
#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
#[derive(Clone, Copy)]
struct JitCodeLoadToken {
	code_len: u32,
	kind: JitCacheKind,
}

#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
#[derive(Clone, Copy)]
enum JitCacheKind {
	Cold,
	Warm,
}

impl JitCodeLoadToken {
	fn cold<T: Config>(info: &CodeInfo<T>) -> Self {
		Self { code_len: info.code_len, kind: JitCacheKind::Cold }
	}

	fn warm<T: Config>(info: &CodeInfo<T>) -> Self {
		Self { code_len: info.code_len, kind: JitCacheKind::Warm }
	}
}

impl<T: Config> Token<T> for JitCodeLoadToken {
	fn weight(&self) -> Weight {
		let (full, base) = match self.kind {
			JitCacheKind::Cold => (
				T::WeightInfo::call_with_pvm_jit_cold_cache_per_byte(self.code_len),
				T::WeightInfo::call_with_pvm_jit_cold_cache_per_byte(0),
			),
			JitCacheKind::Warm => (
				T::WeightInfo::call_with_pvm_jit_warm_cache_per_byte(self.code_len),
				T::WeightInfo::call_with_pvm_jit_warm_cache_per_byte(0),
			),
		};
		full.saturating_sub(base)
	}
}

struct PendingSyscall {
	symbol: Vec<u8>,
	a0: u64,
	a1: u64,
	a2: u64,
	a3: u64,
	a4: u64,
	a5: u64,
}

/// The state of a JIT virtualization instance.
enum JitState {
	/// Idle — ready to be prepared for execution.
	Idle(VirtInstance, ExportedFunction),
	/// Running — prepared and executing (possibly suspended at a syscall).
	Running(VirtExecution),
	/// Terminal — execution finished or errored.
	Done,
}

/// Wraps an [`sp_virtualization::Instance`] into a type that implements
/// [`PolkaVmInstance`].
pub struct JitInstance {
	state: JitState,
	gas: polkavm::Gas,
	pending_syscall: Option<PendingSyscall>,
	pending_a0: u64,
}

impl JitInstance {
	/// Access the running `VirtExecution`. Only valid during syscall handling.
	fn running_execution(&mut self) -> &mut VirtExecution {
		match &mut self.state {
			JitState::Running(execution) => execution,
			_ => panic!(
				"memory access is only called during syscall handling which sets Running state; qed"
			),
		}
	}

	fn map_exec_error<T: Config>(&mut self, error: VirtExecError) -> Interrupt {
		match error {
			VirtExecError::OutOfGas => {
				self.gas = 0;
				Interrupt::OutOfGas
			},
			VirtExecError::Trap => Interrupt::Trap,
			err => {
				log::error!(target: LOG_TARGET, "virt execution error: {err:?}");
				Interrupt::Error(Error::<T>::ExecutionFailed.into())
			},
		}
	}

	fn handle_result<T: Config>(
		&mut self,
		result: VirtExecResult<VirtInstance, VirtExecution>,
	) -> Interrupt {
		match result {
			VirtExecResult::Finished { instance: _, gas_left } => {
				self.gas = gas_left;
				Interrupt::Finished
			},
			VirtExecResult::Syscall {
				execution,
				gas_left,
				syscall_symbol,
				a0,
				a1,
				a2,
				a3,
				a4,
				a5,
			} => {
				self.state = JitState::Running(execution);
				self.gas = gas_left;
				self.pending_syscall = Some(PendingSyscall {
					symbol: syscall_symbol.as_ref().to_vec(),
					a0,
					a1,
					a2,
					a3,
					a4,
					a5,
				});
				Interrupt::Ecalli(0)
			},
			VirtExecResult::Error { instance: _, error } => self.map_exec_error::<T>(error),
		}
	}
}

impl<T: Config> MeterBackend<T> for JitInstance {
	type Backend = JitBackend;
}

impl<T: Config> Memory<T> for JitInstance {
	fn read_into_buf(&mut self, ptr: u32, buf: &mut [u8]) -> Result<(), DispatchError> {
		self.running_execution()
			.read_memory(ptr, buf)
			.map_err(|_| Error::<T>::OutOfBounds.into())
	}

	fn write(&mut self, ptr: u32, buf: &[u8]) -> Result<(), DispatchError> {
		self.running_execution()
			.write_memory(ptr, buf)
			.map_err(|_| Error::<T>::OutOfBounds.into())
	}

	fn zero(&mut self, ptr: u32, len: u32) -> Result<(), DispatchError> {
		self.running_execution()
			.write_memory(ptr, &vec![0u8; len as usize])
			.map_err(|_| Error::<T>::OutOfBounds.into())
	}

	fn reset_interpreter_cache(&mut self) {}
}

impl<T: Config> PolkaVmInstance<T> for JitInstance {
	fn gas(&self) -> polkavm::Gas {
		self.gas
	}

	fn set_gas(&mut self, gas: polkavm::Gas) {
		self.gas = gas;
	}

	fn read_input_regs(&self) -> (u64, u64, u64, u64, u64, u64) {
		match &self.pending_syscall {
			Some(s) => (s.a0, s.a1, s.a2, s.a3, s.a4, s.a5),
			None => (0, 0, 0, 0, 0, 0),
		}
	}

	fn write_output(&mut self, output: u64) {
		self.pending_a0 = output;
	}

	fn run(&mut self) -> Interrupt {
		let state = mem::replace(&mut self.state, JitState::Done);
		match state {
			JitState::Idle(instance, ep) => match instance.prepare(ep.identifier()) {
				Ok(execution) => self.handle_result::<T>(execution.run(self.gas, 0)),
				Err((_instance, err)) => self.map_exec_error::<T>(err),
			},
			JitState::Running(execution) => {
				self.handle_result::<T>(execution.run(self.gas, self.pending_a0))
			},
			JitState::Done => Interrupt::Finished,
		}
	}

	fn resolve_import(&self, _idx: u32) -> Option<&[u8]> {
		Some(self.pending_syscall.as_ref()?.symbol.as_slice())
	}
}

impl<'a, E: Ext> PreparedCall<'a, E, JitInstance> {
	/// Compile (or look up) the host-side JIT module and instantiate it.
	///
	/// The lookup-first cold/warm policy keeps `weight_required` (the
	/// meter's high-water mark) tight: pre-charging cold and refunding to
	/// warm on hit would leave it inflated by `cold - warm` even after
	/// the refund, defeating dry-run estimates.
	///
	/// `blob.code()` is `Some` on the fresh-upload path (`from_pvm_code`) and `None` on the
	/// load-from-storage path, where `PristineCode` is read **in the runtime, on cache miss
	/// only**. Skipping the read on a hit is consensus-safe because the compile cache is
	/// deterministic per block (one fresh cache spans exactly one block execution on
	/// authoring, import and PVF validation alike): a hit implies an earlier call in this
	/// same block already missed, read the code — putting it into the PoV witness — and
	/// compiled it, so the read-set stays identical on all three. Passing the contract's
	/// `storage_key` as the `from_bytes` identifier caches the compiled module under it, so
	/// a later load of the same contract hits [`Module::lookup`].
	pub fn new_jit(
		blob: &ContractBlob<E::T>,
		mut runtime: Runtime<'a, E, JitInstance>,
		entry_point: ExportedFunction,
	) -> Result<Self, ExecError> {
		let code_info = blob.code_info();
		let key = pristine_code::storage_key::<E::T>(blob.code_hash());

		let module = match Module::lookup(&key) {
			Ok(m) => {
				runtime
					.ext()
					.frame_meter_mut()
					.charge_weight_token(JitCodeLoadToken::warm(code_info))?;
				m
			},
			Err(ModuleError::NotCached) => {
				runtime
					.ext()
					.frame_meter_mut()
					.charge_weight_token(JitCodeLoadToken::cold(code_info))?;
				let loaded;
				let code = match blob.code() {
					Some(bytes) => bytes,
					None => {
						loaded = pristine_code::get::<E::T>(blob.code_hash())
							.ok_or(Error::<E::T>::CodeNotFound)?;
						&loaded
					},
				};
				Module::from_bytes(code, Some(&key)).map_err(|err| -> DispatchError {
					log::debug!(target: LOG_TARGET, "jit compile failed: {err:?}");
					Error::<E::T>::CodeRejected.into()
				})?
			},
			Err(err) => {
				log::debug!(target: LOG_TARGET, "jit Module::lookup failed: {err:?}");
				return Err(Error::<E::T>::CodeRejected.into());
			},
		};

		let gas = runtime.ext().frame_meter_mut().sync_to_executor::<JitBackend>();
		let virt_instance = module.instantiate().map_err(|err| {
			log::debug!(target: LOG_TARGET, "failed to instantiate: {err:?}");
			Error::<E::T>::CodeRejected
		})?;
		Ok(Self {
			instance: JitInstance {
				state: JitState::Idle(virt_instance, entry_point),
				gas,
				pending_syscall: None,
				pending_a0: 0,
			},
			runtime,
		})
	}
}
