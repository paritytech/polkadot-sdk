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

//! This module contains the cost schedule and supporting code that constructs a
//! sane default schedule from a `WeightInfo` implementation.

use crate::{weights::WeightInfo, Config};

use codec::{Decode, Encode};
use core::marker::PhantomData;
use frame_support::DefaultNoBound;
use scale_info::TypeInfo;
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

/// Definition of the cost schedule and other parameterizations for the wasm vm.
///
/// Its [`Default`] implementation is the designated way to initialize this type. It uses
/// the benchmarked information supplied by [`Config::WeightInfo`]. All of its fields are
/// public and can therefore be modified. For example in order to change some of the limits
/// and set a custom instruction weight version the following code could be used:
/// ```rust
/// use pallet_contracts::{Schedule, Limits, InstructionWeights, Config};
///
/// fn create_schedule<T: Config>() -> Schedule<T> {
///     Schedule {
///         limits: Limits {
/// 		        memory_pages: 16,
/// 		        .. Default::default()
/// 	        },
///         instruction_weights: InstructionWeights {
///             .. Default::default()
///         },
/// 	        .. Default::default()
///     }
/// }
/// ```
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "std", serde(bound(serialize = "", deserialize = "")))]
#[cfg_attr(feature = "runtime-benchmarks", derive(frame_support::DebugNoBound))]
#[derive(Clone, Encode, Decode, PartialEq, Eq, DefaultNoBound, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct Schedule<T: Config> {
	/// Describes the upper limits on various metrics.
	pub limits: Limits,

	/// The weights for individual wasm instructions.
	pub instruction_weights: InstructionWeights<T>,
}

/// Describes the upper limits on various metrics.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "runtime-benchmarks", derive(Debug))]
#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo)]
pub struct Limits {
	/// The maximum number of topics supported by an event.
	pub event_topics: u32,

	/// Maximum number of memory pages allowed for a contract.
	pub memory_pages: u32,

	/// The maximum length of a subject in bytes used for PRNG generation.
	pub subject_len: u32,

	/// The maximum size of a storage value and event payload in bytes.
	pub payload_len: u32,

	/// The maximum node runtime memory. This is for integrity checks only and does not affect the
	/// real setting.
	pub runtime_memory: u32,
}

impl Limits {
	/// The maximum memory size in bytes that a contract can occupy.
	pub fn max_memory_size(&self) -> u32 {
		self.memory_pages * 64 * 1024
	}
}

/// Gas metering of Wasm executed instructions is being done on the engine side.
/// This struct holds a reference value used to gas units scaling between host and engine.
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "runtime-benchmarks", derive(frame_support::DebugNoBound))]
#[derive(Clone, Encode, Decode, PartialEq, Eq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct InstructionWeights<T: Config> {
	/// Base instruction `ref_time` Weight.
	/// Should match to wasmi's `1` fuel (see <https://github.com/paritytech/wasmi/issues/701>).
	pub base: u32,
	/// The type parameter is used in the default implementation.
	#[codec(skip)]
	pub _phantom: PhantomData<T>,
}

impl Default for Limits {
	fn default() -> Self {
		Self {
			event_topics: 4,
			memory_pages: 16,
			subject_len: 32,
			payload_len: 16 * 1024,
			runtime_memory: 1024 * 1024 * 128,
		}
	}
}

impl<T: Config> Default for InstructionWeights<T> {
	/// We execute 6 different instructions therefore we have to divide the actual
	/// computed gas costs by 6 to have a rough estimate as to how expensive each
	/// single executed instruction is going to be.
	fn default() -> Self {
		let instr_cost = T::WeightInfo::instr_i64_load_store(1)
			.saturating_sub(T::WeightInfo::instr_i64_load_store(0))
			.ref_time() as u32;
		let base = instr_cost / 6;
		Self { base, _phantom: PhantomData }
	}
}
