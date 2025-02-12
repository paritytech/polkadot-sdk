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
use crate::*;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};

/// Debug Ethererum JSON-RPC apis.
#[rpc(server, client)]
pub trait DebugRpc {
	/// Returns the tracing of the execution of a specific block using its number.
	///
	/// ## References
	///
	/// - <https://geth.ethereum.org/docs/interacting-with-geth/rpc/ns-debug#debugtraceblockbynumb>er
	#[method(name = "debug_traceBlockByNumber")]
	async fn trace_block_by_number(
		&self,
		block: BlockNumberOrTag,
		tracer_config: TracerConfig,
	) -> RpcResult<Vec<TransactionTrace>>;

	/// Returns a transaction's traces by replaying it.
	///
	/// ## References
	///
	/// - <https://geth.ethereum.org/docs/interacting-with-geth/rpc/ns-debug#debugtracetransaction>
	#[method(name = "debug_traceTransaction")]
	async fn trace_transaction(
		&self,
		transaction_hash: H256,
		tracer_config: TracerConfig,
	) -> RpcResult<CallTrace>;

	/// Dry run a call and returns the transaction's traces.
	///
	/// ## References
	///
	/// - <https://geth.ethereum.org/docs/interacting-with-geth/rpc/ns-debug#debugtracecall>
	#[method(name = "debug_traceCall")]
	async fn trace_call(
		&self,
		transaction: GenericTransaction,
		block: BlockNumberOrTag,
		tracer_config: TracerConfig,
	) -> RpcResult<CallTrace>;
}

pub struct DebugRpcServerImpl {
	client: client::Client,
}

impl DebugRpcServerImpl {
	pub fn new(client: client::Client) -> Self {
		Self { client }
	}
}

#[async_trait]
impl DebugRpcServer for DebugRpcServerImpl {
	async fn trace_block_by_number(
		&self,
		block: BlockNumberOrTag,
		tracer_config: TracerConfig,
	) -> RpcResult<Vec<TransactionTrace>> {
		log::debug!(target: crate::LOG_TARGET, "trace_block_by_number: {block:?} config: {tracer_config:?}");
		let traces = self.client.trace_block_by_number(block, tracer_config).await?;
		Ok(traces)
	}

	async fn trace_transaction(
		&self,
		transaction_hash: H256,
		tracer_config: TracerConfig,
	) -> RpcResult<CallTrace> {
		let trace = self.client.trace_transaction(transaction_hash, tracer_config).await?;
		Ok(trace)
	}

	async fn trace_call(
		&self,
		transaction: GenericTransaction,
		block: BlockNumberOrTag,
		tracer_config: TracerConfig,
	) -> RpcResult<CallTrace> {
		log::debug!(target: crate::LOG_TARGET, "trace_call: {transaction:?} block: {block:?} config: {tracer_config:?}");
		let trace = self.client.trace_call(transaction, block, tracer_config).await?;
		Ok(trace)
	}
}
