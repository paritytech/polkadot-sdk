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

//! Abstract execution environment parameter set.
//!
//! Parameter set is encoded as an opaque vector which structure depends on the execution
//! environment itself (except for environment type/version which is always represented
//! by the first element of the vector). Decoding to a usable semantics structure is
//! done in `polkadot-node-core-pvf`.

use crate::{BlakeTwo256, HashT as _, PvfExecKind, PvfPrepKind};
use parity_scale_codec::{Decode, Encode};
use polkadot_core_primitives::Hash;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{collections::btree_map::BTreeMap, ops::Deref, time::Duration, vec, vec::Vec};

/// Default maximum number of wasm values allowed for the stack during execution of a PVF.
pub const DEFAULT_LOGICAL_STACK_MAX: u32 = 65536;
/// Default maximum number of bytes devoted for the stack during execution of a PVF.
pub const DEFAULT_NATIVE_STACK_MAX: u32 = 256 * 1024 * 1024;

/// The limit of [`ExecutorParam::MaxMemoryPages`].
pub const MEMORY_PAGES_MAX: u32 = 65536;
/// The lower bound of [`ExecutorParam::StackLogicalMax`].
pub const LOGICAL_MAX_LO: u32 = 1024;
/// The upper bound of [`ExecutorParam::StackLogicalMax`].
pub const LOGICAL_MAX_HI: u32 = 2 * 65536;
/// The lower bound of [`ExecutorParam::PrecheckingMaxMemory`].
pub const PRECHECK_MEM_MAX_LO: u64 = 256 * 1024 * 1024;
/// The upper bound of [`ExecutorParam::PrecheckingMaxMemory`].
pub const PRECHECK_MEM_MAX_HI: u64 = 16 * 1024 * 1024 * 1024;

// Default PVF timeouts. Must never be changed! Use executor environment parameters to adjust them.
// See also `PvfPrepKind` and `PvfExecKind` docs.

/// Default PVF preparation timeout for prechecking requests.
pub const DEFAULT_PRECHECK_PREPARATION_TIMEOUT: Duration = Duration::from_secs(60);
/// Default PVF preparation timeout for execution requests.
pub const DEFAULT_LENIENT_PREPARATION_TIMEOUT: Duration = Duration::from_secs(360);
/// Default PVF execution timeout for backing.
pub const DEFAULT_BACKING_EXECUTION_TIMEOUT: Duration = Duration::from_secs(2);
/// Default PVF execution timeout for approval or disputes.
pub const DEFAULT_APPROVAL_EXECUTION_TIMEOUT: Duration = Duration::from_secs(12);

const DEFAULT_PRECHECK_PREPARATION_TIMEOUT_MS: u64 =
	DEFAULT_PRECHECK_PREPARATION_TIMEOUT.as_millis() as u64;
const DEFAULT_LENIENT_PREPARATION_TIMEOUT_MS: u64 =
	DEFAULT_LENIENT_PREPARATION_TIMEOUT.as_millis() as u64;
const DEFAULT_BACKING_EXECUTION_TIMEOUT_MS: u64 =
	DEFAULT_BACKING_EXECUTION_TIMEOUT.as_millis() as u64;
const DEFAULT_APPROVAL_EXECUTION_TIMEOUT_MS: u64 =
	DEFAULT_APPROVAL_EXECUTION_TIMEOUT.as_millis() as u64;

/// The different executor parameters for changing the execution environment semantics.
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, TypeInfo, Serialize, Deserialize)]
pub enum ExecutorParam {
	/// Maximum number of memory pages (64KiB bytes per page) the executor can allocate.
	/// A valid value lies within (0, 65536].
	#[codec(index = 1)]
	MaxMemoryPages(u32),
	/// Wasm logical stack size limit (max. number of Wasm values on stack).
	/// A valid value lies within [[`LOGICAL_MAX_LO`], [`LOGICAL_MAX_HI`]].
	///
	/// For WebAssembly, the stack limit is subject to implementations, meaning that it may vary on
	/// different platforms. However, we want execution to be deterministic across machines of
	/// different architectures, including failures like stack overflow. For deterministic
	/// overflow, we rely on a **logical** limit, the maximum number of values allowed to be pushed
	/// on the stack.
	#[codec(index = 2)]
	StackLogicalMax(u32),
	/// Executor machine stack size limit, in bytes.
	/// If `StackLogicalMax` is also present, a valid value should not fall below
	/// 128 * `StackLogicalMax`.
	///
	/// For deterministic overflow, `StackLogicalMax` should be reached before the native stack is
	/// exhausted.
	#[codec(index = 3)]
	StackNativeMax(u32),
	/// Max. amount of memory the preparation worker is allowed to use during
	/// pre-checking, in bytes.
	/// Valid max. memory ranges from [`PRECHECK_MEM_MAX_LO`] to [`PRECHECK_MEM_MAX_HI`].
	#[codec(index = 4)]
	PrecheckingMaxMemory(u64),
	/// PVF preparation timeouts, in millisecond.
	/// Always ensure that `precheck_timeout` < `lenient_timeout`.
	/// When absent, the default values will be used.
	#[codec(index = 5)]
	PvfPrepTimeout(PvfPrepKind, u64),
	/// PVF execution timeouts, in millisecond.
	/// Always ensure that `backing_timeout` < `approval_timeout`.
	/// When absent, the default values will be used.
	#[codec(index = 6)]
	PvfExecTimeout(PvfExecKind, u64),
	/// Enables WASM bulk memory proposal
	#[codec(index = 7)]
	WasmExtBulkMemory,
}

/// Possible inconsistencies of executor params.
#[derive(Debug)]
pub enum ExecutorParamError {
	/// A param is duplicated.
	DuplicatedParam(&'static str),
	/// A param value exceeds its limitation.
	OutsideLimit(&'static str),
	/// Two param values are incompatible or senseless when put together.
	IncompatibleValues(&'static str, &'static str),
}

/// Unit type wrapper around [`type@Hash`] that represents an execution parameter set hash.
///
/// This type is produced by [`ExecutorParams::hash`].
#[derive(Clone, Copy, Encode, Decode, Hash, Eq, PartialEq, PartialOrd, Ord, TypeInfo)]
pub struct ExecutorParamsHash(Hash);

impl ExecutorParamsHash {
	/// Create a new executor parameter hash from `H256` hash
	pub fn from_hash(hash: Hash) -> Self {
		Self(hash)
	}
}

impl sp_std::fmt::Display for ExecutorParamsHash {
	fn fmt(&self, f: &mut sp_std::fmt::Formatter<'_>) -> sp_std::fmt::Result {
		self.0.fmt(f)
	}
}

impl sp_std::fmt::Debug for ExecutorParamsHash {
	fn fmt(&self, f: &mut sp_std::fmt::Formatter<'_>) -> sp_std::fmt::Result {
		write!(f, "{:?}", self.0)
	}
}

impl sp_std::fmt::LowerHex for ExecutorParamsHash {
	fn fmt(&self, f: &mut sp_std::fmt::Formatter<'_>) -> sp_std::fmt::Result {
		sp_std::fmt::LowerHex::fmt(&self.0, f)
	}
}

/// # Deterministically serialized execution environment semantics
/// Represents an arbitrary semantics of an arbitrary execution environment, so should be kept as
/// abstract as possible.
// ADR: For mandatory entries, mandatoriness should be enforced in code rather than separating them
// into individual fields of the structure. Thus, complex migrations shall be avoided when adding
// new entries and removing old ones. At the moment, there's no mandatory parameters defined. If
// they show up, they must be clearly documented as mandatory ones.
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, TypeInfo, Serialize, Deserialize)]
pub struct ExecutorParams(Vec<ExecutorParam>);

impl ExecutorParams {
	/// Creates a new, empty executor parameter set
	pub fn new() -> Self {
		ExecutorParams(vec![])
	}

	/// Returns hash of the set of execution environment parameters
	pub fn hash(&self) -> ExecutorParamsHash {
		ExecutorParamsHash(BlakeTwo256::hash(&self.encode()))
	}

	/// Returns a PVF preparation timeout, if any
	pub fn pvf_prep_timeout(&self, kind: PvfPrepKind) -> Option<Duration> {
		for param in &self.0 {
			if let ExecutorParam::PvfPrepTimeout(k, timeout) = param {
				if kind == *k {
					return Some(Duration::from_millis(*timeout))
				}
			}
		}
		None
	}

	/// Returns a PVF execution timeout, if any
	pub fn pvf_exec_timeout(&self, kind: PvfExecKind) -> Option<Duration> {
		for param in &self.0 {
			if let ExecutorParam::PvfExecTimeout(k, timeout) = param {
				if kind == *k {
					return Some(Duration::from_millis(*timeout))
				}
			}
		}
		None
	}

	/// Returns pre-checking memory limit, if any
	pub fn prechecking_max_memory(&self) -> Option<u64> {
		for param in &self.0 {
			if let ExecutorParam::PrecheckingMaxMemory(limit) = param {
				return Some(*limit)
			}
		}
		None
	}

	/// Check params coherence.
	pub fn check_consistency(&self) -> Result<(), ExecutorParamError> {
		use ExecutorParam::*;
		use ExecutorParamError::*;

		let mut seen = BTreeMap::<&str, u64>::new();

		macro_rules! check {
			($param:ident, $val:expr $(,)?) => {
				if seen.contains_key($param) {
					return Err(DuplicatedParam($param))
				}
				seen.insert($param, $val as u64);
			};

			// should check existence before range
			($param:ident, $val:expr, $out_of_limit:expr $(,)?) => {
				if seen.contains_key($param) {
					return Err(DuplicatedParam($param))
				}
				if $out_of_limit {
					return Err(OutsideLimit($param))
				}
				seen.insert($param, $val as u64);
			};
		}

		for param in &self.0 {
			// should ensure to be unique
			let param_ident = match *param {
				MaxMemoryPages(_) => "MaxMemoryPages",
				StackLogicalMax(_) => "StackLogicalMax",
				StackNativeMax(_) => "StackNativeMax",
				PrecheckingMaxMemory(_) => "PrecheckingMaxMemory",
				PvfPrepTimeout(kind, _) => match kind {
					PvfPrepKind::Precheck => "PvfPrepKind::Precheck",
					PvfPrepKind::Prepare => "PvfPrepKind::Prepare",
				},
				PvfExecTimeout(kind, _) => match kind {
					PvfExecKind::Backing => "PvfExecKind::Backing",
					PvfExecKind::Approval => "PvfExecKind::Approval",
				},
				WasmExtBulkMemory => "WasmExtBulkMemory",
			};

			match *param {
				MaxMemoryPages(val) => {
					check!(param_ident, val, val == 0 || val > MEMORY_PAGES_MAX,);
				},

				StackLogicalMax(val) => {
					check!(param_ident, val, val < LOGICAL_MAX_LO || val > LOGICAL_MAX_HI,);
				},

				StackNativeMax(val) => {
					check!(param_ident, val);
				},

				PrecheckingMaxMemory(val) => {
					check!(
						param_ident,
						val,
						val < PRECHECK_MEM_MAX_LO || val > PRECHECK_MEM_MAX_HI,
					);
				},

				PvfPrepTimeout(_, val) => {
					check!(param_ident, val);
				},

				PvfExecTimeout(_, val) => {
					check!(param_ident, val);
				},

				WasmExtBulkMemory => {
					check!(param_ident, 1);
				},
			}
		}

		if let (Some(lm), Some(nm)) = (
			seen.get("StackLogicalMax").or(Some(&(DEFAULT_LOGICAL_STACK_MAX as u64))),
			seen.get("StackNativeMax").or(Some(&(DEFAULT_NATIVE_STACK_MAX as u64))),
		) {
			if *nm < 128 * *lm {
				return Err(IncompatibleValues("StackLogicalMax", "StackNativeMax"))
			}
		}

		if let (Some(precheck), Some(lenient)) = (
			seen.get("PvfPrepKind::Precheck")
				.or(Some(&DEFAULT_PRECHECK_PREPARATION_TIMEOUT_MS)),
			seen.get("PvfPrepKind::Prepare")
				.or(Some(&DEFAULT_LENIENT_PREPARATION_TIMEOUT_MS)),
		) {
			if *precheck >= *lenient {
				return Err(IncompatibleValues("PvfPrepKind::Precheck", "PvfPrepKind::Prepare"))
			}
		}

		if let (Some(backing), Some(approval)) = (
			seen.get("PvfExecKind::Backing").or(Some(&DEFAULT_BACKING_EXECUTION_TIMEOUT_MS)),
			seen.get("PvfExecKind::Approval")
				.or(Some(&DEFAULT_APPROVAL_EXECUTION_TIMEOUT_MS)),
		) {
			if *backing >= *approval {
				return Err(IncompatibleValues("PvfExecKind::Backing", "PvfExecKind::Approval"))
			}
		}

		Ok(())
	}
}

impl Deref for ExecutorParams {
	type Target = Vec<ExecutorParam>;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl From<&[ExecutorParam]> for ExecutorParams {
	fn from(arr: &[ExecutorParam]) -> Self {
		ExecutorParams(arr.to_vec())
	}
}

impl Default for ExecutorParams {
	fn default() -> Self {
		ExecutorParams(vec![])
	}
}
