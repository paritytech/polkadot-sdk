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
		path = "sp_runtime::generic::block::Block<A, B>",
		with = "::subxt::utils::Static<::sp_runtime::generic::Block<
		::sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>,
		::sp_runtime::OpaqueExtrinsic
		>>"
	),

	substitute_type(
		path = "pallet_revive::primitives::EthTransactError",
		with = "::subxt::utils::Static<::pallet_revive::EthTransactError>"
	),
	substitute_type(
		path = "sp_weights::weight_v2::Weight",
		with = "::subxt::utils::Static<::sp_weights::Weight>"
	),
	substitute_type(
		path = "pallet_revive::evm::api::block::Block",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::BlockV1>"
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
		path = "pallet_revive_types::runtime_api::types::receipt::ReceiptGasInfoV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::ReceiptGasInfoV1>"
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
		path = "pallet_revive_types::runtime_api::types::transaction::GenericTransactionV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::GenericTransactionV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::transaction::InputOrDataV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::InputOrDataV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::transaction::AccessListEntryV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::AccessListEntryV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::transaction::AuthorizationListEntryV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::AuthorizationListEntryV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::state_overrides::StateOverrideSetV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::StateOverrideSetV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::state_overrides::StateOverrideV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::StateOverrideV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::state_overrides::StorageOverrideV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::StorageOverrideV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::state_overrides::TracingConfigV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TracingConfigV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::dry_run::EthTransactInfoV1<Balance>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::EthTransactInfoV1<Balance>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::dry_run::DryRunConfigV1<Moment>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::DryRunConfigV1<Moment>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::block::BlockV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::BlockV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::block::WithdrawalV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::WithdrawalV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::transaction::HashesOrTransactionInfosV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::HashesOrTransactionInfosV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::transaction::TransactionInfoV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TransactionInfoV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::transaction::TransactionSignedV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TransactionSignedV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::transaction::Transaction1559UnsignedV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::Transaction1559UnsignedV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::transaction::Transaction2930UnsignedV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::Transaction2930UnsignedV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::transaction::Transaction4844UnsignedV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::Transaction4844UnsignedV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::transaction::TransactionLegacyUnsignedV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TransactionLegacyUnsignedV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::transaction::Transaction7702UnsignedV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::Transaction7702UnsignedV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::transaction::Transaction7702SignedV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::Transaction7702SignedV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::transaction::Transaction1559SignedV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::Transaction1559SignedV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::transaction::Transaction2930SignedV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::Transaction2930SignedV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::transaction::Transaction4844SignedV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::Transaction4844SignedV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::transaction::TransactionLegacySignedV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TransactionLegacySignedV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::contract::ContractResultV1<R, Balance>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::ContractResultV1<R, Balance>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::contract::StorageDepositV1<Balance>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::StorageDepositV1<Balance>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::contract::ExecReturnValueV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::ExecReturnValueV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::contract::InstantiateReturnValueV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::InstantiateReturnValueV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::contract::CodeV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::CodeV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::eth_block::BlockInputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::BlockInputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::eth_block::BlockVersionedInputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::BlockVersionedInputPayload>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::eth_block::BlockOutputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::BlockOutputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::eth_block::BlockVersionedOutputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::BlockVersionedOutputPayload>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::balance::BalanceInputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::BalanceInputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::balance::BalanceVersionedInputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::BalanceVersionedInputPayload>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::balance::BalanceOutputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::BalanceOutputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::balance::BalanceVersionedOutputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::BalanceVersionedOutputPayload>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::gas_price::GasPriceInputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::GasPriceInputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::gas_price::GasPriceVersionedInputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::GasPriceVersionedInputPayload>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::gas_price::GasPriceOutputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::GasPriceOutputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::gas_price::GasPriceVersionedOutputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::GasPriceVersionedOutputPayload>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::nonce::NonceInputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::NonceInputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::nonce::NonceVersionedInputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::NonceVersionedInputPayload>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::nonce::NonceOutputPayloadV1<Nonce>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::NonceOutputPayloadV1<Nonce>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::nonce::NonceVersionedOutputPayload<Nonce>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::NonceVersionedOutputPayload<Nonce>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::call::CallInputPayloadV1<AccountId, Balance>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::CallInputPayloadV1<AccountId, Balance>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::call::CallVersionedInputPayload<AccountId, Balance>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::CallVersionedInputPayload<AccountId, Balance>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::call::CallOutputPayloadV1<Balance>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::CallOutputPayloadV1<Balance>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::call::CallVersionedOutputPayload<Balance>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::CallVersionedOutputPayload<Balance>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::instantiate::InstantiateInputPayloadV1<AccountId, Balance>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::InstantiateInputPayloadV1<AccountId, Balance>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::instantiate::InstantiateVersionedInputPayload<AccountId, Balance>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::InstantiateVersionedInputPayload<AccountId, Balance>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::instantiate::InstantiateOutputPayloadV1<Balance>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::InstantiateOutputPayloadV1<Balance>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::instantiate::InstantiateVersionedOutputPayload<Balance>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::InstantiateVersionedOutputPayload<Balance>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::eth_transact::TransactInputPayloadV1<Moment>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TransactInputPayloadV1<Moment>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::eth_transact::TransactVersionedInputPayload<Moment>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TransactVersionedInputPayload<Moment>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::eth_transact::TransactOutputPayloadV1<Balance>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TransactOutputPayloadV1<Balance>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::eth_transact::TransactVersionedOutputPayload<Balance>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TransactVersionedOutputPayload<Balance>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::eth_estimate_gas::EstimateGasInputPayloadV1<Moment>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::EstimateGasInputPayloadV1<Moment>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::eth_estimate_gas::EstimateGasVersionedInputPayload<Moment>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::EstimateGasVersionedInputPayload<Moment>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::eth_estimate_gas::EstimateGasOutputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::EstimateGasOutputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::eth_estimate_gas::EstimateGasVersionedOutputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::EstimateGasVersionedOutputPayload>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::eth_pre_dispatch_weight::PreDispatchWeightInputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::PreDispatchWeightInputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::eth_pre_dispatch_weight::PreDispatchWeightVersionedInputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::PreDispatchWeightVersionedInputPayload>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::eth_pre_dispatch_weight::PreDispatchWeightOutputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::PreDispatchWeightOutputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::eth_pre_dispatch_weight::PreDispatchWeightVersionedOutputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::PreDispatchWeightVersionedOutputPayload>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::upload::CodeUploadReturnValueV1<Balance>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::CodeUploadReturnValueV1<Balance>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::upload_code::UploadCodeInputPayloadV1<AccountId, Balance>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::UploadCodeInputPayloadV1<AccountId, Balance>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::upload_code::UploadCodeVersionedInputPayload<AccountId, Balance>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::UploadCodeVersionedInputPayload<AccountId, Balance>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::upload_code::UploadCodeOutputPayloadV1<Balance>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::UploadCodeOutputPayloadV1<Balance>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::upload_code::UploadCodeVersionedOutputPayload<Balance>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::UploadCodeVersionedOutputPayload<Balance>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::types::storage::StorageKeyV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::StorageKeyV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::get_storage::GetStorageInputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::GetStorageInputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::get_storage::GetStorageVersionedInputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::GetStorageVersionedInputPayload>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::get_storage::GetStorageOutputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::GetStorageOutputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::get_storage::GetStorageVersionedOutputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::GetStorageVersionedOutputPayload>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::runtime_pallets_address::RuntimePalletsAddressInputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::RuntimePalletsAddressInputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::runtime_pallets_address::RuntimePalletsAddressVersionedInputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::RuntimePalletsAddressVersionedInputPayload>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::runtime_pallets_address::RuntimePalletsAddressOutputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::RuntimePalletsAddressOutputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::runtime_pallets_address::RuntimePalletsAddressVersionedOutputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::RuntimePalletsAddressVersionedOutputPayload>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::code::CodeInputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::CodeInputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::code::CodeVersionedInputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::CodeVersionedInputPayload>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::code::CodeOutputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::CodeOutputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::code::CodeVersionedOutputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::CodeVersionedOutputPayload>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::account_id::AccountIdInputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::AccountIdInputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::account_id::AccountIdVersionedInputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::AccountIdVersionedInputPayload>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::account_id::AccountIdOutputPayloadV1<AccountId>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::AccountIdOutputPayloadV1<AccountId>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::account_id::AccountIdVersionedOutputPayload<AccountId>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::AccountIdVersionedOutputPayload<AccountId>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::new_balance_with_dust::NewBalanceWithDustInputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::NewBalanceWithDustInputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::new_balance_with_dust::NewBalanceWithDustVersionedInputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::NewBalanceWithDustVersionedInputPayload>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::new_balance_with_dust::NewBalanceWithDustOutputPayloadV1<Balance>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::NewBalanceWithDustOutputPayloadV1<Balance>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::new_balance_with_dust::NewBalanceWithDustVersionedOutputPayload<Balance>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::NewBalanceWithDustVersionedOutputPayload<Balance>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::block_author::BlockAuthorInputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::BlockAuthorInputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::block_author::BlockAuthorVersionedInputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::BlockAuthorVersionedInputPayload>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::block_author::BlockAuthorOutputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::BlockAuthorOutputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::block_author::BlockAuthorVersionedOutputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::BlockAuthorVersionedOutputPayload>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::address::AddressInputPayloadV1<AccountId>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::AddressInputPayloadV1<AccountId>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::address::AddressVersionedInputPayload<AccountId>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::AddressVersionedInputPayload<AccountId>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::address::AddressOutputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::AddressOutputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::address::AddressVersionedOutputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::AddressVersionedOutputPayload>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::block_gas_limit::BlockGasLimitInputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::BlockGasLimitInputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::block_gas_limit::BlockGasLimitVersionedInputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::BlockGasLimitVersionedInputPayload>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::block_gas_limit::BlockGasLimitOutputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::BlockGasLimitOutputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::block_gas_limit::BlockGasLimitVersionedOutputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::BlockGasLimitVersionedOutputPayload>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::max_extrinsic_weight_in_gas::MaxExtrinsicWeightInGasInputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::MaxExtrinsicWeightInGasInputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::max_extrinsic_weight_in_gas::MaxExtrinsicWeightInGasVersionedInputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::MaxExtrinsicWeightInGasVersionedInputPayload>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::max_extrinsic_weight_in_gas::MaxExtrinsicWeightInGasOutputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::MaxExtrinsicWeightInGasOutputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::max_extrinsic_weight_in_gas::MaxExtrinsicWeightInGasVersionedOutputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::MaxExtrinsicWeightInGasVersionedOutputPayload>"
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
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::trace_tx::TraceTxInputPayloadV1<Block>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TraceTxInputPayloadV1<Block>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::trace_tx::TraceTxInputPayloadV2<Block>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TraceTxInputPayloadV2<Block>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::trace_tx::TraceTxVersionedInputPayload<Block>",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TraceTxVersionedInputPayload<Block>>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::trace_tx::TraceTxOutputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TraceTxOutputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::trace_tx::TraceTxOutputPayloadV2",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TraceTxOutputPayloadV2>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::trace_tx::TraceTxVersionedOutputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TraceTxVersionedOutputPayload>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::trace_call::TraceCallInputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TraceCallInputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::trace_call::TraceCallInputPayloadV2",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TraceCallInputPayloadV2>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::trace_call::TraceCallVersionedInputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TraceCallVersionedInputPayload>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::trace_call::TraceCallOutputPayloadV1",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TraceCallOutputPayloadV1>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::trace_call::TraceCallOutputPayloadV2",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TraceCallOutputPayloadV2>"
	),
	substitute_type(
		path = "pallet_revive_types::runtime_api::payloads::trace_call::TraceCallVersionedOutputPayload",
		with = "::subxt::utils::Static<::pallet_revive_types::runtime_api::TraceCallVersionedOutputPayload>"
	),

	derive_for_all_types = "codec::Encode, codec::Decode"
)]
mod src_chain {}
pub use src_chain::*;

use crate::{
	BlockId,
	block_info_provider::{BlockInfo, BlockInfoProvider},
	client::{Balance, ClientError, SubscriptionType, SubstrateBlockHash, SubstrateBlockNumber},
	substrate_client::{NodeHealth, RawExtrinsic, SubmitResult, SubstrateClient},
};
use futures::TryStreamExt;
use jsonrpsee::core::async_trait;
use pallet_revive::evm::U256;
use pallet_revive_types::runtime_api::{
	BlockV1 as EthBlock, EthTransactInfoV1 as EthTransactInfo,
	GenericTransactionV1 as GenericTransaction, ReceiptGasInfoV1,
	StateOverrideSetV1 as StateOverrideSet, TraceV1, TracerTypeV1,
};
use sp_core::H256;
use sp_weights::Weight;
use std::{sync::Arc, time::Duration};
use subxt::{
	OnlineClient,
	backend::{StreamOf, StreamOfResults},
	client::OnlineClientAtBlock,
	config::RpcConfigFor,
	error::OnlineClientAtBlockError,
	rpcs::{
		RpcClient,
		methods::{LegacyRpcMethods, legacy::TransactionStatus as SubxtTxStatus},
		rpc_params,
	},
};
use tokio::sync::RwLock;

/// A handle to a substrate block at a specific height (subxt-backed).
pub type SubstrateBlock = OnlineClientAtBlock<SrcChainConfig>;

/// The substrate block header.
pub type SubstrateBlockHeader = <SrcChainConfig as subxt::Config>::Header;

fn system_health_from_subxt(h: subxt::rpcs::methods::legacy::SystemHealth) -> NodeHealth {
	NodeHealth { peers: h.peers, is_syncing: h.is_syncing, should_have_peers: h.should_have_peers }
}

/// A lightweight block-info value returned by [`SubxtClient`].
#[derive(Clone, Debug)]
pub struct SubxtBlockInfo {
	pub hash: SubstrateBlockHash,
	pub number: SubstrateBlockNumber,
	pub parent_hash: SubstrateBlockHash,
}

impl BlockInfo for SubxtBlockInfo {
	fn hash(&self) -> H256 {
		self.hash
	}

	fn number(&self) -> SubstrateBlockNumber {
		self.number
	}

	fn parent_hash(&self) -> H256 {
		self.parent_hash
	}
}

/// Materialize a lightweight [`SubxtBlockInfo`] from a subxt block handle.
///
/// In subxt 0.50 a [`SubstrateBlock`] (`OnlineClientAtBlock`) is a lazy handle whose header is
/// fetched asynchronously, so we snapshot the fields the generic client needs (which are exposed
/// synchronously through the [`BlockInfo`] trait) up-front.
async fn block_info_from_subxt(block: &SubstrateBlock) -> Result<SubxtBlockInfo, ClientError> {
	let header = block.block_header().await?;
	Ok(SubxtBlockInfo {
		hash: block.block_hash(),
		// The source runtime uses `u32` block numbers (see the metadata substitution above); subxt
		// exposes them as `u64`, so narrow back to the native `SubstrateBlockNumber`.
		number: header.number as SubstrateBlockNumber,
		parent_hash: header.parent_hash,
	})
}

/// The subxt-backed block info provider.
#[derive(Clone)]
pub struct SubxtBlockInfoProvider {
	/// The latest block.
	latest_block: Arc<RwLock<Arc<SubxtBlockInfo>>>,

	/// The latest finalized block.
	latest_finalized_block: Arc<RwLock<Arc<SubxtBlockInfo>>>,

	/// The rpc client, used to fetch blocks not in the cache.
	rpc: LegacyRpcMethods<RpcConfigFor<SrcChainConfig>>,

	/// The api client, used to fetch blocks not in the cache.
	api: OnlineClient<SrcChainConfig>,
}

impl SubxtBlockInfoProvider {
	pub async fn new(
		api: OnlineClient<SrcChainConfig>,
		rpc: LegacyRpcMethods<RpcConfigFor<SrcChainConfig>>,
	) -> Result<Self, ClientError> {
		let block = api.at_current_block().await?;
		let latest = Arc::new(block_info_from_subxt(&block).await?);
		Ok(Self {
			api,
			rpc,
			latest_block: Arc::new(RwLock::new(latest.clone())),
			latest_finalized_block: Arc::new(RwLock::new(latest)),
		})
	}
}

#[async_trait]
impl BlockInfoProvider for SubxtBlockInfoProvider {
	type Block = SubxtBlockInfo;

	async fn update_latest(&self, block: Arc<SubxtBlockInfo>, subscription_type: SubscriptionType) {
		let mut latest = match subscription_type {
			SubscriptionType::FinalizedBlocks => self.latest_finalized_block.write().await,
			SubscriptionType::BestBlocks => self.latest_block.write().await,
		};
		*latest = block;
	}

	async fn latest_block(&self) -> Arc<SubxtBlockInfo> {
		self.latest_block.read().await.clone()
	}

	async fn latest_finalized_block(&self) -> Arc<SubxtBlockInfo> {
		self.latest_finalized_block.read().await.clone()
	}

	async fn block_by_number(
		&self,
		block_number: SubstrateBlockNumber,
	) -> Result<Option<Arc<SubxtBlockInfo>>, ClientError> {
		let latest = self.latest_block().await;
		if block_number == latest.number() {
			return Ok(Some(latest));
		}

		let latest_finalized = self.latest_finalized_block().await;
		if block_number == latest_finalized.number() {
			return Ok(Some(latest_finalized));
		}

		let Some(hash) = self.rpc.chain_get_block_hash(Some(block_number.into())).await? else {
			return Ok(None);
		};

		match self.api.at_block(hash).await {
			Ok(block) => Ok(Some(Arc::new(block_info_from_subxt(&block).await?))),
			Err(
				OnlineClientAtBlockError::BlockHeaderNotFound { .. } |
				OnlineClientAtBlockError::BlockNotFound { .. },
			) => Ok(None),
			Err(err) => Err(err.into()),
		}
	}

	async fn block_by_hash(&self, hash: &H256) -> Result<Option<Arc<SubxtBlockInfo>>, ClientError> {
		let latest = self.latest_block().await;
		if hash == &latest.hash() {
			return Ok(Some(latest));
		}

		let latest_finalized = self.latest_finalized_block().await;
		if hash == &latest_finalized.hash() {
			return Ok(Some(latest_finalized));
		}

		match self.api.at_block(*hash).await {
			Ok(block) => Ok(Some(Arc::new(block_info_from_subxt(&block).await?))),
			Err(
				OnlineClientAtBlockError::BlockHeaderNotFound { .. } |
				OnlineClientAtBlockError::BlockNotFound { .. },
			) => Ok(None),
			Err(err) => Err(err.into()),
		}
	}
}

/// [`SubstrateClient`] implementation backed by a `subxt` WebSocket connection.
#[derive(Clone)]
pub struct SubxtClient {
	pub(crate) api: OnlineClient<SrcChainConfig>,
	pub(crate) rpc_client: RpcClient,
	pub(crate) rpc: LegacyRpcMethods<RpcConfigFor<SrcChainConfig>>,
	pub(crate) chain_id: u64,
	pub(crate) max_block_weight: Weight,
	pub(crate) pallet_revive_index: u8,
}

impl SubxtClient {
	pub async fn new(
		api: OnlineClient<SrcChainConfig>,
		rpc_client: RpcClient,
		rpc: LegacyRpcMethods<RpcConfigFor<SrcChainConfig>>,
	) -> Result<Self, ClientError> {
		let at_block = api.at_current_block().await?;

		let chain_id = {
			let query = constants().revive().chain_id().unvalidated();
			at_block.constants().entry(query)?
		};

		let max_block_weight = {
			let query = constants().system().block_weights().unvalidated();
			let weights = at_block.constants().entry(query)?;
			let max_block = weights.per_class.normal.max_extrinsic.unwrap_or(weights.max_block);
			max_block.0
		};

		let pallet_revive_index = at_block
			.metadata()
			.pallet_by_name("Revive")
			.map(|p| p.call_index())
			.unwrap_or_else(|| {
				log::warn!(
					target: crate::LOG_TARGET,
					"Revive pallet not found in subxt metadata; \
					 falling back to default pallet index 253."
				);
				253
			});

		Ok(Self { api, rpc_client, rpc, chain_id, max_block_weight, pallet_revive_index })
	}
}

#[async_trait]
impl SubstrateClient for SubxtClient {
	type BlockInfo = SubxtBlockInfo;

	fn chain_id(&self) -> u64 {
		self.chain_id
	}

	fn max_block_weight(&self) -> Weight {
		self.max_block_weight
	}

	async fn block_by_hash(
		&self,
		hash: &SubstrateBlockHash,
	) -> Result<Option<SubxtBlockInfo>, ClientError> {
		match self.api.at_block(*hash).await {
			Ok(b) => Ok(Some(block_info_from_subxt(&b).await?)),
			Err(
				OnlineClientAtBlockError::BlockHeaderNotFound { .. } |
				OnlineClientAtBlockError::BlockNotFound { .. },
			) => Ok(None),
			Err(e) => Err(e.into()),
		}
	}

	async fn block_by_number(
		&self,
		number: SubstrateBlockNumber,
	) -> Result<Option<SubxtBlockInfo>, ClientError> {
		let Some(hash) = self.rpc.chain_get_block_hash(Some(number.into())).await? else {
			return Ok(None);
		};
		self.block_by_hash(&hash).await
	}

	async fn latest_block(&self) -> Result<SubxtBlockInfo, ClientError> {
		let block = self.api.at_current_block().await?;
		block_info_from_subxt(&block).await
	}

	async fn latest_finalized_block(&self) -> Result<SubxtBlockInfo, ClientError> {
		let hash = self.rpc.chain_get_finalized_head().await?;
		let block = self.api.at_block(hash).await?;
		block_info_from_subxt(&block).await
	}

	async fn dry_run(
		&self,
		block_hash: SubstrateBlockHash,
		tx: GenericTransaction,
		block: BlockId,
		state_overrides: Option<StateOverrideSet>,
	) -> Result<EthTransactInfo<Balance>, ClientError> {
		use crate::client::runtime_api::RuntimeApi;
		RuntimeApi::new(self.api.at_block(block_hash).await?)
			.dry_run(tx, block, state_overrides)
			.await
	}

	async fn estimate_gas(
		&self,
		block_hash: SubstrateBlockHash,
		tx: GenericTransaction,
		block: BlockId,
	) -> Result<U256, ClientError> {
		use crate::client::runtime_api::RuntimeApi;
		RuntimeApi::new(self.api.at_block(block_hash).await?)
			.estimate_gas(tx, block)
			.await
	}

	async fn gas_price(&self, block_hash: SubstrateBlockHash) -> Result<U256, ClientError> {
		use crate::client::runtime_api::RuntimeApi;
		RuntimeApi::new(self.api.at_block(block_hash).await?).gas_price().await
	}

	async fn balance(
		&self,
		block_hash: SubstrateBlockHash,
		address: sp_core::H160,
	) -> Result<U256, ClientError> {
		use crate::client::runtime_api::RuntimeApi;
		RuntimeApi::new(self.api.at_block(block_hash).await?).balance(address).await
	}

	async fn nonce(
		&self,
		block_hash: SubstrateBlockHash,
		address: sp_core::H160,
	) -> Result<U256, ClientError> {
		use crate::client::runtime_api::RuntimeApi;
		RuntimeApi::new(self.api.at_block(block_hash).await?).nonce(address).await
	}

	async fn code(
		&self,
		block_hash: SubstrateBlockHash,
		address: sp_core::H160,
	) -> Result<Vec<u8>, ClientError> {
		use crate::client::runtime_api::RuntimeApi;
		RuntimeApi::new(self.api.at_block(block_hash).await?).code(address).await
	}

	async fn get_storage(
		&self,
		block_hash: SubstrateBlockHash,
		address: sp_core::H160,
		key: [u8; 32],
	) -> Result<Option<Vec<u8>>, ClientError> {
		use crate::client::runtime_api::RuntimeApi;
		RuntimeApi::new(self.api.at_block(block_hash).await?)
			.get_storage(address, key)
			.await
	}

	async fn eth_block(&self, block_hash: SubstrateBlockHash) -> Result<EthBlock, ClientError> {
		use crate::client::runtime_api::RuntimeApi;
		RuntimeApi::new(self.api.at_block(block_hash).await?).eth_block().await
	}

	async fn eth_block_hash(
		&self,
		block_hash: SubstrateBlockHash,
		number: U256,
	) -> Result<Option<H256>, ClientError> {
		use crate::client::runtime_api::RuntimeApi;
		RuntimeApi::new(self.api.at_block(block_hash).await?)
			.eth_block_hash(number)
			.await
	}

	async fn eth_receipt_data(
		&self,
		block_hash: SubstrateBlockHash,
	) -> Result<Vec<ReceiptGasInfoV1>, ClientError> {
		use crate::client::runtime_api::RuntimeApi;
		RuntimeApi::new(self.api.at_block(block_hash).await?).eth_receipt_data().await
	}

	async fn trace_block(
		&self,
		_block_hash: SubstrateBlockHash,
		block: sp_runtime::generic::Block<
			sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>,
			sp_runtime::OpaqueExtrinsic,
		>,
		config: TracerTypeV1,
	) -> Result<Vec<(u32, TraceV1)>, ClientError> {
		use crate::client::runtime_api::RuntimeApi;
		let parent = block.header.parent_hash;
		RuntimeApi::new(self.api.at_block(parent).await?)
			.trace_block(block, config)
			.await
	}

	async fn trace_tx(
		&self,
		_block_hash: SubstrateBlockHash,
		block: sp_runtime::generic::Block<
			sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>,
			sp_runtime::OpaqueExtrinsic,
		>,
		transaction_index: u32,
		config: TracerTypeV1,
	) -> Result<TraceV1, ClientError> {
		use crate::client::runtime_api::RuntimeApi;
		let parent = block.header.parent_hash;
		RuntimeApi::new(self.api.at_block(parent).await?)
			.trace_tx(block, transaction_index, config)
			.await
	}

	async fn trace_call(
		&self,
		block_hash: SubstrateBlockHash,
		transaction: GenericTransaction,
		config: TracerTypeV1,
		state_overrides: Option<StateOverrideSet>,
	) -> Result<TraceV1, ClientError> {
		use crate::client::runtime_api::RuntimeApi;
		RuntimeApi::new(self.api.at_block(block_hash).await?)
			.trace_call(transaction, config, state_overrides)
			.await
	}

	async fn submit_extrinsic(&self, payload: Vec<u8>) -> Result<SubmitResult, ClientError> {
		let call = tx().revive().eth_transact(payload);
		let at_block = self.api.at_current_block().await?;
		let ext = at_block.tx().create_unsigned(&call)?;

		let sub = self
			.rpc_client
			.subscribe(
				"author_submitAndWatchExtrinsic",
				rpc_params![to_hex(ext.encoded())],
				"author_unwatchExtrinsic",
			)
			.await?;

		let mut stream: StreamOfResults<SubxtTxStatus<SubstrateBlockHash>> =
			StreamOf::new(Box::pin(sub.map_err(|e| e.into())));

		tokio::time::timeout(std::time::Duration::from_secs(5), async {
			if let Some(status) = stream.next().await {
				let subxt_status: SubxtTxStatus<SubstrateBlockHash> =
					status.map_err(ClientError::from)?;
				let pool_status = subxt_tx_status_to_submit_result(subxt_status);

				match pool_status {
					SubmitResult::Usurped(_) | SubmitResult::Dropped | SubmitResult::Invalid => {
						return Err(ClientError::SubmitError(crate::client::SubmitError::from(
							pool_status,
						)));
					},
					other => return Ok(other),
				}
			}
			Err(ClientError::SubmitError(crate::client::SubmitError::StreamEnded))
		})
		.await
		.map_err(|_| ClientError::Timeout)?
	}

	async fn sync_state(
		&self,
	) -> Result<sc_rpc::system::SyncState<SubstrateBlockNumber>, ClientError> {
		let sync_state: sc_rpc::system::SyncState<SubstrateBlockNumber> =
			self.rpc_client.request("system_syncState", Default::default()).await?;
		Ok(sync_state)
	}

	async fn system_health(&self) -> Result<NodeHealth, ClientError> {
		Ok(system_health_from_subxt(self.rpc.system_health().await?))
	}

	async fn get_automine(&self) -> bool {
		self.rpc_client
			.request::<bool>("getAutomine", rpc_params![])
			.await
			.unwrap_or(false)
	}

	async fn subscribe_blocks(
		&self,
		subscription_type: SubscriptionType,
	) -> Result<tokio::sync::mpsc::Receiver<SubxtBlockInfo>, ClientError> {
		let mut block_stream = match subscription_type {
			SubscriptionType::BestBlocks => self.api.stream_best_blocks().await,
			SubscriptionType::FinalizedBlocks => self.api.stream_blocks().await,
		}?;

		let (tx, rx) = tokio::sync::mpsc::channel(crate::client::NOTIFIER_CAPACITY);
		tokio::spawn(async move {
			while let Some(block) = block_stream.next().await {
				let block = match block {
					Ok(b) => b,
					Err(err) => {
						let err = subxt::Error::from(err);
						if err.is_disconnected_will_reconnect() {
							log::warn!(
								target: crate::LOG_TARGET,
								"RPC connection lost ({subscription_type:?}): {err:?}"
							);
							continue;
						}
						log::error!(
							target: crate::LOG_TARGET,
							"block subscription failed ({subscription_type:?}): {err:?}"
						);
						break;
					},
				};

				let block = match block.at().await {
					Ok(b) => b,
					Err(err) => {
						log::error!(
							target: crate::LOG_TARGET,
							"failed to resolve block ({subscription_type:?}): {err:?}"
						);
						continue;
					},
				};
				let info = match block_info_from_subxt(&block).await {
					Ok(info) => info,
					Err(err) => {
						log::error!(
							target: crate::LOG_TARGET,
							"failed to build block info ({subscription_type:?}): {err:?}"
						);
						continue;
					},
				};

				if tx.send(info).await.is_err() {
					break;
				}
			}
		});

		Ok(rx)
	}

	async fn signed_block(
		&self,
		block_hash: SubstrateBlockHash,
	) -> Result<
		sp_runtime::generic::Block<
			sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>,
			sp_runtime::OpaqueExtrinsic,
		>,
		ClientError,
	> {
		let signed: sp_runtime::generic::SignedBlock<
			sp_runtime::generic::Block<
				sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>,
				sp_runtime::OpaqueExtrinsic,
			>,
		> = self.rpc_client.request("chain_getBlock", rpc_params![block_hash]).await?;
		Ok(signed.block)
	}

	async fn block_extrinsics(
		&self,
		block_hash: SubstrateBlockHash,
	) -> Result<Vec<RawExtrinsic>, ClientError> {
		let block = self.api.at_block(block_hash).await?;
		let extrinsics = block.extrinsics().fetch().await?;
		extrinsics
			.iter()
			.enumerate()
			.map(|(index, ext)| {
				let ext = ext?;
				Ok(RawExtrinsic { payload: ext.bytes().to_vec(), index })
			})
			.collect()
	}

	async fn block_events(
		&self,
		block_hash: SubstrateBlockHash,
	) -> Result<Option<crate::receipt_extractor::BlockEvents>, ClientError> {
		use revive::events::{ContractEmitted, EthExtrinsicRevert};
		use subxt::events::{DecodeAsEvent as _, Phase};

		let block = match self.api.at_block(block_hash).await {
			Ok(b) => b,
			Err(
				OnlineClientAtBlockError::BlockHeaderNotFound { .. } |
				OnlineClientAtBlockError::BlockNotFound { .. },
			) => return Ok(None),
			Err(e) => return Err(e.into()),
		};

		let extrinsics = block.extrinsics().fetch().await?;
		let num_extrinsics = extrinsics.iter().count();
		let events = block.events().fetch().await?;

		let mut block_events: crate::receipt_extractor::BlockEvents = (0..num_extrinsics)
			.map(|_| crate::receipt_extractor::ExtrinsicEvents { success: true, logs: vec![] })
			.collect();

		for event_result in events.iter() {
			let event = match event_result {
				Ok(e) => e,
				Err(_) => continue,
			};

			let extrinsic_idx = match event.phase() {
				Phase::ApplyExtrinsic(idx) => idx as usize,
				_ => continue,
			};

			if extrinsic_idx >= block_events.len() {
				continue;
			}

			let pallet_name = event.pallet_name();
			let event_name = event.event_name();

			if EthExtrinsicRevert::is_event(pallet_name, event_name) {
				block_events[extrinsic_idx].success = false;
			} else if ContractEmitted::is_event(pallet_name, event_name) {
				if let Some(Ok(evt)) = event.decode_fields_as::<ContractEmitted>() {
					block_events[extrinsic_idx].logs.push((
						evt.contract,
						evt.topics,
						Some(evt.data),
					));
				}
			}
		}

		Ok(Some(block_events))
	}

	async fn extrinsic_post_dispatch_weight(
		&self,
		block_hash: SubstrateBlockHash,
		extrinsic_index: usize,
	) -> Option<sp_weights::Weight> {
		use system::events::ExtrinsicSuccess;
		let block = self.api.at_block(block_hash).await.ok()?;
		let extrinsics = block.extrinsics().fetch().await.ok()?;
		let ext = extrinsics.iter().nth(extrinsic_index)?.ok()?;
		let event = ext.events().await.ok()?.find_first::<ExtrinsicSuccess>()?.ok()?;
		Some(event.dispatch_info.weight.0)
	}

	fn pallet_revive_index(&self) -> u8 {
		self.pallet_revive_index
	}
}

fn to_hex(bytes: impl AsRef<[u8]>) -> String {
	format!("0x{}", hex::encode(bytes.as_ref()))
}

fn subxt_tx_status_to_submit_result(s: SubxtTxStatus<SubstrateBlockHash>) -> SubmitResult {
	use sc_transaction_pool_api::TransactionStatus as Pool;
	match s {
		SubxtTxStatus::Future => Pool::Future,
		SubxtTxStatus::Ready => Pool::Ready,
		SubxtTxStatus::Broadcast(peers) => Pool::Broadcast(peers),
		SubxtTxStatus::InBlock(hash) => Pool::InBlock((hash, 0)),
		SubxtTxStatus::Retracted(hash) => Pool::Retracted(hash),
		SubxtTxStatus::FinalityTimeout(hash) => Pool::FinalityTimeout(hash),
		SubxtTxStatus::Finalized(hash) => Pool::Finalized((hash, 0)),
		SubxtTxStatus::Usurped(hash) => Pool::Usurped(hash),
		SubxtTxStatus::Dropped => Pool::Dropped,
		SubxtTxStatus::Invalid => Pool::Invalid,
	}
}

use subxt::rpcs::client::reconnecting_rpc_client::{
	ExponentialBackoff, RpcClient as ReconnectingRpcClient,
};

pub async fn connect(
	node_rpc_url: &str,
	max_request_size: u32,
	max_response_size: u32,
) -> Result<
	(OnlineClient<SrcChainConfig>, RpcClient, LegacyRpcMethods<RpcConfigFor<SrcChainConfig>>),
	ClientError,
> {
	log::info!(target: crate::LOG_TARGET, "🌐 Connecting to node at: {node_rpc_url} ...");
	let rpc_client = ReconnectingRpcClient::builder()
		.retry_policy(ExponentialBackoff::from_millis(100).max_delay(Duration::from_secs(10)))
		.max_request_size(max_request_size)
		.max_response_size(max_response_size)
		.build(node_rpc_url.to_string())
		.await?;
	let rpc_client = RpcClient::new(rpc_client);
	log::info!(target: crate::LOG_TARGET, "🌟 Connected to node at: {node_rpc_url}");

	let api = OnlineClient::<SrcChainConfig>::from_rpc_client(rpc_client.clone()).await?;
	let rpc = LegacyRpcMethods::<RpcConfigFor<SrcChainConfig>>::new(rpc_client.clone());
	Ok((api, rpc_client, rpc))
}
