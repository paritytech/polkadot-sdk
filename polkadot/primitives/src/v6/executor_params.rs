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

use crate::{BlakeTwo256, HashT as _, PvfExecTimeoutKind, PvfPrepTimeoutKind};
use parity_scale_codec::{Decode, Encode};
use polkadot_core_primitives::Hash;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{ops::Deref, time::Duration, vec, vec::Vec};

const MEMORY_PAGES_LIMIT: u32 = 65536;
const SHADOW_STACK_PAGES: u32 = 32;

/// The different executor parameters for changing the execution environment semantics.
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, TypeInfo, Serialize, Deserialize)]
pub enum ExecutorParam {
	/// Maximum number of memory pages (64KiB bytes per page) the executor can allocate.
	/// A valid value lies within (0, 65536 + 32].
	#[codec(index = 1)]
	MaxMemoryPages(u32),
	/// Wasm logical stack size limit (max. number of Wasm values on stack).
	/// A valid value lies within [1024, 2 * 65536].
	#[codec(index = 2)]
	StackLogicalMax(u32),
	/// Executor machine stack size limit, in bytes.
	/// If `StackLogicalMax` is also present, a valid value lies within
	/// [128 * logical_max, 512 * logical_max].
	#[codec(index = 3)]
	StackNativeMax(u32),
	/// Max. amount of memory the preparation worker is allowed to use during
	/// pre-checking, in bytes.
	/// A valid max memory ranges from 256MB to 16GB.
	#[codec(index = 4)]
	PrecheckingMaxMemory(u64),
	/// PVF preparation timeouts, millisec
	/// If both `PvfPrepTimeoutKind::Precheck` and `PvfPrepTimeoutKind::Lenient` are present,
	/// ensure that `precheck` < `lenient`.
	#[codec(index = 5)]
	PvfPrepTimeout(PvfPrepTimeoutKind, u64),
	/// PVF execution timeouts, millisec
	/// If both `PvfExecTimeoutKind::Backing` and `PvfExecTimeoutKind::Approval` are present,
	/// ensure that `backing` < `approval`.
	#[codec(index = 6)]
	PvfExecTimeout(PvfExecTimeoutKind, u64),
	/// Enables WASM bulk memory proposal
	#[codec(index = 7)]
	WasmExtBulkMemory,
}

#[derive(Debug)]
pub enum ExecutorParamError {
	DuplicatedParam(&'static str),
	LimitExceeded(&'static str),
	IncompatibleValues(&'static str, &'static str)
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
	pub fn pvf_prep_timeout(&self, kind: PvfPrepTimeoutKind) -> Option<Duration> {
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
	pub fn pvf_exec_timeout(&self, kind: PvfExecTimeoutKind) -> Option<Duration> {
		for param in &self.0 {
			if let ExecutorParam::PvfExecTimeout(k, timeout) = param {
				if kind == *k {
					return Some(Duration::from_millis(*timeout))
				}
			}
		}
		None
	}

	// FIXME Should it be
	// pub fn check_consistency(&self) -> Result<(), ExecutorParamError>, which would be simpler.
	// I guess it depends on whether they could be considered "warnings".

	/// Check params coherence
	pub fn check_consistency(&self) -> Vec<ExecutorParamError> {
		use ExecutorParam::*;
		use ExecutorParamError::*;

		let mut errors = Vec::with_capacity(8);

		let mut max_pages = false;
		let mut logical_max: Option<u32> = None;
		let mut native_max: Option<u32> = None;
		let mut pvf_mem_max = false;
		let mut pvf_prep_precheck: Option<u64> = None;
		let mut pvf_prep_lenient: Option<u64> = None;
		let mut pvf_exec_backing: Option<u64> = None;
		let mut pvf_exec_approval: Option<u64> = None;
		let mut enable_bulk_mem = false;

		for param in &self.0 {
			let param_ident = match *param {
				MaxMemoryPages(_) => "MaxMemoryPages",
				StackLogicalMax(_) => "StackLogicalMax",
				StackNativeMax(_) => "StackNativeMax",
				PrecheckingMaxMemory(_) => "PrecheckingMaxMemory",
				PvfPrepTimeout(kind, _) => match kind {
					PvfPrepTimeoutKind::Precheck => "PvfPrepTimeoutKind::Precheck",
					PvfPrepTimeoutKind::Lenient => "PvfPrepTimeoutKind::Lenient",
				},
				PvfExecTimeout(kind, _) => match kind {
					PvfExecTimeoutKind::Backing => "PvfExecTimeoutKind::Backing",
					PvfExecTimeoutKind::Approval => "PvfExecTimeoutKind::Approval",
				},
				WasmExtBulkMemory => "WasmExtBulkMemory",
			};

			// FIXME report each kind of duplication only once
			match *param {
				MaxMemoryPages(max) => if max_pages {
					errors.push(DuplicatedParam(param_ident));
				} else {
					max_pages = true;
					if max <= 0 || max > MEMORY_PAGES_LIMIT + SHADOW_STACK_PAGES {
						errors.push(LimitExceeded(param_ident));
					}
				}

				StackLogicalMax(max) => if logical_max.is_some() {
					errors.push(DuplicatedParam(param_ident));
				} else {
					logical_max = Some(max);
					if max < 1024 || max > 2 * 65536 {
						errors.push(LimitExceeded(param_ident));
					}
				}

				StackNativeMax(max) => if native_max.is_some() {
					errors.push(DuplicatedParam(param_ident));
				} else {
					native_max = Some(max);
				}

				// FIXME upper bound
				PrecheckingMaxMemory(max) => if pvf_mem_max {
					errors.push(DuplicatedParam(param_ident));
				} else {
					pvf_mem_max = true;
					if max < 256 * 1024 * 1024 || max > 16 * 1024 * 1024 * 1024 {
						errors.push(LimitExceeded(param_ident));
					}
				}

				// FIXME upper bounds
				PvfPrepTimeout(kind, timeout) => match kind {
					PvfPrepTimeoutKind::Precheck => if pvf_prep_precheck.is_some() {
						errors.push(DuplicatedParam(param_ident));
					} else {
						pvf_prep_precheck = Some(timeout);
					}
					PvfPrepTimeoutKind::Lenient => if pvf_prep_lenient.is_some() {
						errors.push(DuplicatedParam(param_ident));
					} else {
						pvf_prep_lenient = Some(timeout);
					}
				}

				// FIXME upper bounds
				PvfExecTimeout(kind, timeout) => match kind {
					PvfExecTimeoutKind::Backing => if pvf_exec_backing.is_some() {
						errors.push(DuplicatedParam(param_ident));
					} else {
						pvf_exec_backing = Some(timeout);
					}
					PvfExecTimeoutKind::Approval => if pvf_exec_approval.is_some() {
						errors.push(DuplicatedParam(param_ident));
					} else {
						pvf_exec_approval = Some(timeout);
					}
				}

				WasmExtBulkMemory => if enable_bulk_mem {
					errors.push(DuplicatedParam(param_ident));
				} else {
					enable_bulk_mem = true;
				}
			}

			// FIXME is it valid if only one is present?
			if let (Some(lm), Some(nm)) = (logical_max, native_max) {
				if nm < 128 * lm || nm > 512 * lm {
					errors.push(IncompatibleValues("StackLogicalMax", "StackNativeMax"));
				}
			}

			if let (Some(precheck), Some(lenient)) = (pvf_prep_precheck, pvf_prep_lenient) {
				if precheck >= lenient {
					errors.push(IncompatibleValues("PvfPrepTimeoutKind::Precheck", "PvfPrepTimeoutKind::Lenient"));
				}
			}

			if let (Some(backing), Some(approval)) = (pvf_exec_backing, pvf_exec_approval) {
				if backing >= approval {
					errors.push(IncompatibleValues("PvfExecTimeoutKind::Backing", "PvfExecTimeoutKind::Approval"));
				}
			}
		}

		errors
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
