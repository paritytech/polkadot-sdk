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
//! The generated subxt client.
//! Generated against a substrate chain configured with [`pallet_revive`] using:
//! subxt metadata  --url ws://localhost:9944 -o rpc/revive_chain.scale
pub use subxt::config::PolkadotConfig as SrcChainConfig;

#[subxt::subxt(
	runtime_metadata_path = "revive_chain.metadata",
	// TODO remove once subxt use the same U256 type
	substitute_type(
		path = "primitive_types::U256",
		with = "::subxt::utils::Static<::sp_core::U256>"
	),

	substitute_type(
		path = "sp_runtime::generic::block::Block<A, B, C, D, E>",
		with = "::subxt::utils::Static<::sp_runtime::generic::Block<
		::sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>,
		::sp_runtime::OpaqueExtrinsic
		>>"
	),
	substitute_type(
		path = "pallet_revive::evm::api::debug_rpc_types::Trace",
		with = "::subxt::utils::Static<::pallet_revive::evm::Trace>"
	),
	substitute_type(
		path = "pallet_revive::evm::api::debug_rpc_types::TracerType",
		with = "::subxt::utils::Static<::pallet_revive::evm::TracerType>"
	),

	substitute_type(
		path = "pallet_revive::evm::api::rpc_types_gen::GenericTransaction",
		with = "::subxt::utils::Static<::pallet_revive::evm::GenericTransaction>"
	),
	substitute_type(
		path = "pallet_revive::primitives::EthTransactInfo<B>",
		with = "::subxt::utils::Static<::pallet_revive::EthTransactInfo<B>>"
	),
	substitute_type(
		path = "pallet_revive::primitives::EthTransactError",
		with = "::subxt::utils::Static<::pallet_revive::EthTransactError>"
	),
	substitute_type(
		path = "pallet_revive::primitives::ExecReturnValue",
		with = "::subxt::utils::Static<::pallet_revive::ExecReturnValue>"
	),
	substitute_type(
		path = "sp_weights::weight_v2::Weight",
		with = "::subxt::utils::Static<::sp_weights::Weight>"
	)
)]
mod src_chain {}
pub use src_chain::*;
