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
use sp_std::{collections::btree_map::BTreeMap, ops::Deref, time::Duration, vec, vec::Vec};

const MEMORY_PAGES_MAX: u32 = 65536;
const LOGICAL_MAX_LO: u32 = 1024;
const LOGICAL_MAX_HI: u32 = 2 * 65536;
const PRECHECK_MEM_MAX_LO: u64 = 256 * 1024 * 1024;
const PRECHECK_MEM_MAX_HI: u64 = 16 * 1024 * 1024 * 1024;

/// The different executor parameters for changing the execution environment semantics.
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, TypeInfo, Serialize, Deserialize)]
pub enum ExecutorParam {
	/// Maximum number of memory pages (64KiB bytes per page) the executor can allocate.
	/// A valid value lies within (0, 65536].
	#[codec(index = 1)]
	MaxMemoryPages(u32),
	/// Wasm logical stack size limit (max. number of Wasm values on stack).
	/// A valid value lies within [1024, 2 * 65536].
	#[codec(index = 2)]
	StackLogicalMax(u32),
	/// Executor machine stack size limit, in bytes.
	/// If `StackLogicalMax` is also present, a valid value should not fall below
	/// 128 * `logical_max`.
	#[codec(index = 3)]
	StackNativeMax(u32),
	/// Max. amount of memory the preparation worker is allowed to use during
	/// pre-checking, in bytes.
	/// Valid max. memory ranges from 256MB to 16GB.
	#[codec(index = 4)]
	PrecheckingMaxMemory(u64),
	/// PVF preparation timeouts, in millisecond.
	/// Always ensure that `precheck_timeout` < `lenient_timeout`.
	/// If not set, the default values will be used, 60,000 and 360,000 respectively.
	#[codec(index = 5)]
	PvfPrepTimeout(PvfPrepTimeoutKind, u64),
	/// PVF execution timeouts, in millisecond.
	/// Always ensure that `backing_timeout` < `approval_timeout`.
	/// If not set, the default values will be used, 2,000 and 12,000 respectively.
	#[codec(index = 6)]
	PvfExecTimeout(PvfExecTimeoutKind, u64),
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
	LimitExceeded(&'static str),
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
	pub fn pvf_prep_timeout(&self, kind: PvfPrepTimeoutKind) -> Option<Duration> {
		for param in &self.0 {
			if let ExecutorParam::PvfPrepTimeout(k, timeout) = param {
				if kind == *k {
					return Some(Duration::from_millis(*timeout));
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
					return Some(Duration::from_millis(*timeout));
				}
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
					return Err(DuplicatedParam($param));
				}
				seen.insert($param, $val as u64);
			};

			// should check existence before range
			($param:ident, $val:expr, $out_of_limit:expr $(,)?) => {
				if seen.contains_key($param) {
					return Err(DuplicatedParam($param));
				}
				if $out_of_limit {
					return Err(LimitExceeded($param));
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
					PvfPrepTimeoutKind::Precheck => "PvfPrepTimeoutKind::Precheck",
					PvfPrepTimeoutKind::Lenient => "PvfPrepTimeoutKind::Lenient",
				},
				PvfExecTimeout(kind, _) => match kind {
					PvfExecTimeoutKind::Backing => "PvfExecTimeoutKind::Backing",
					PvfExecTimeoutKind::Approval => "PvfExecTimeoutKind::Approval",
				},
				WasmExtBulkMemory => "WasmExtBulkMemory",
			};

			match *param {
				MaxMemoryPages(val) => {
					check!(param_ident, val, val <= 0 || val > MEMORY_PAGES_MAX,);
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
					// 1 is a dummy for inserting the key into the map
					check!(param_ident, 1);
				},
			}

			// FIXME is it valid if only one is present?
			if let (Some(lm), Some(nm)) = (seen.get("StackLogicalMax"), seen.get("StackNativeMax"))
			{
				if *nm < 128 * *lm {
					return Err(IncompatibleValues("StackLogicalMax", "StackNativeMax"));
				}
			}

			match (
				seen.get("PvfPrepTimeoutKind::Precheck"),
				seen.get("PvfPrepTimeoutKind::Lenient"),
			) {
				(Some(precheck), Some(lenient)) if *precheck >= *lenient => {
					return Err(IncompatibleValues(
						"PvfPrepTimeoutKind::Precheck",
						"PvfPrepTimeoutKind::Lenient",
					));
				},

				(Some(precheck), None) if *precheck >= 360000 => {
					return Err(IncompatibleValues(
						"PvfPrepTimeoutKind::Precheck",
						"PvfPrepTimeoutKind::Lenient default",
					));
				},

				(None, Some(lenient)) if *lenient <= 60000 => {
					return Err(IncompatibleValues(
						"PvfPrepTimeoutKind::Precheck default",
						"PvfPrepTimeoutKind::Lenient",
					));
				},

				(_, _) => {},
			}

			match (
				seen.get("PvfExecTimeoutKind::Backing"),
				seen.get("PvfExecTimeoutKind::Approval"),
			) {
				(Some(backing), Some(approval)) if *backing >= *approval => {
					return Err(IncompatibleValues(
						"PvfExecTimeoutKind::Backing",
						"PvfExecTimeoutKind::Approval",
					));
				},

				(Some(backing), None) if *backing >= 12000 => {
					return Err(IncompatibleValues(
						"PvfExecTimeoutKind::Backing",
						"PvfExecTimeoutKind::Approval default",
					));
				},

				(None, Some(approval)) if *approval <= 2000 => {
					return Err(IncompatibleValues(
						"PvfExecTimeoutKind::Backing default",
						"PvfExecTimeoutKind::Approval",
					));
				},

				(_, _) => {},
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
