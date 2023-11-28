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

//! Provides executor related types intended to be used across Substrate node.

/// Host functions required for kitchensink runtime and Substrate node.
#[cfg(not(feature = "runtime-benchmarks"))]
pub type HostFunctions =
	(sp_io::SubstrateHostFunctions, sp_statement_store::runtime_api::HostFunctions);

#[cfg(feature = "runtime-benchmarks")]
pub type HostFunctions = (
	sp_io::SubstrateHostFunctions,
	sp_statement_store::runtime_api::HostFunctions,
	frame_benchmarking::benchmarking::HostFunctions,
);

/// A specialized `WasmExecutor` intended to use accross substrate node. It provides all required
/// HostFunctions.
pub type RuntimeExecutor = sc_executor::WasmExecutor<HostFunctions>;
