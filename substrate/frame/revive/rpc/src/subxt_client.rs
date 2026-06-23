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
	runtime_metadata_path = "$OUT_DIR/revive_chain.scale",
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
		path = "pallet_revive::evm::api::rpc_types_gen::GenericTransaction",
		with = "::subxt::utils::Static<::pallet_revive::evm::GenericTransaction>"
	),
	substitute_type(
		path = "pallet_revive::evm::api::rpc_types::DryRunConfig<M>",
		with = "::subxt::utils::Static<::pallet_revive::evm::DryRunConfig<M>>"
	),
	substitute_type(
		path = "pallet_revive::evm::api::rpc_types::TracingConfig",
		with = "::subxt::utils::Static<::pallet_revive::evm::TracingConfig>"
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
	),
	substitute_type(
		path = "pallet_revive::evm::api::rpc_types_gen::Block",
		with = "::subxt::utils::Static<::pallet_revive::evm::Block>"
	),
	substitute_type(
		path = "pallet_revive::evm::block_hash::ReceiptGasInfo",
		with = "::subxt::utils::Static<::pallet_revive::evm::ReceiptGasInfo>"
	),

	// Versioning replacements
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::tracer::TracerTypeV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TracerTypeV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::tracer::CallTracerConfigV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::CallTracerConfigV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::tracer::PrestateTracerConfigV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::PrestateTracerConfigV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::tracer::ExecutionTracerConfigV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::ExecutionTracerConfigV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::traces::TraceV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TraceV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::traces::TraceV2",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TraceV2>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::traces::CallTraceV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::CallTraceV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::traces::CallTraceV2",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::CallTraceV2>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::traces::PrestateTraceV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::PrestateTraceV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::traces::ExecutionTraceV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::ExecutionTraceV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::traces::CallLogV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::CallLogV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::traces::CallLogV2",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::CallLogV2>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::traces::CallTypeV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::CallTypeV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::traces::PrestateTraceInfoV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::PrestateTraceInfoV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::traces::ExecutionStepV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::ExecutionStepV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::traces::ExecutionStepKindV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::ExecutionStepKindV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::trace_block::TraceBlockInputPayloadV1<Block>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TraceBlockInputPayloadV1<Block>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::trace_block::TraceBlockInputPayloadV2<Block>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TraceBlockInputPayloadV2<Block>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::trace_block::TraceBlockVersionedInputPayload<Block>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TraceBlockVersionedInputPayload<Block>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::trace_block::TraceBlockOutputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TraceBlockOutputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::trace_block::TraceBlockOutputPayloadV2",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TraceBlockOutputPayloadV2>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::trace_block::TraceBlockVersionedOutputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TraceBlockVersionedOutputPayload>"
	),

	derive_for_all_types = "codec::Encode, codec::Decode"
)]
mod src_chain {}
pub use src_chain::*;
