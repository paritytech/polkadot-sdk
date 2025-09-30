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
use crate::{
	evm::{CallTrace, OpcodeTrace, Trace},
	tracing::Tracing,
	BalanceOf, Bounded, Config, Error, MomentOf, Weight,
};
use alloc::string::String;
use sp_core::{H256, U256};
use sp_runtime::DispatchError;

mod call_tracing;
pub use call_tracing::*;

mod prestate_tracing;
pub use prestate_tracing::*;

mod opcode_tracing;
pub use opcode_tracing::*;

/// A composite tracer.
#[derive(derive_more::From, Debug)]
pub enum Tracer<T> {
	/// A tracer that traces calls.
	CallTracer(CallTracer<U256, fn(Weight) -> U256>),
	/// A tracer that traces the prestate.
	PrestateTracer(PrestateTracer<T>),
	/// A tracer that traces opcodes.
	OpcodeTracer(OpcodeTracer<U256, fn(Weight) -> U256>),
}

impl<T: Config> Tracer<T>
where
	BalanceOf<T>: Into<U256> + TryFrom<U256> + Bounded,
	MomentOf<T>: Into<U256>,
	T::Hash: frame_support::traits::IsType<H256>,
	T::Nonce: Into<u32>,
{
	/// Returns an empty trace.
	pub fn empty_trace(&self) -> Trace {
		match self {
			Tracer::CallTracer(_) => CallTrace::default().into(),
			Tracer::PrestateTracer(tracer) => tracer.empty_trace().into(),
			Tracer::OpcodeTracer(_) => OpcodeTrace::default().into(),
		}
	}

	/// Get a mutable trait‐object reference to the inner tracer.
	pub fn as_tracing(&mut self) -> &mut (dyn Tracing + 'static) {
		match self {
			Tracer::CallTracer(inner) => inner as &mut dyn Tracing,
			Tracer::PrestateTracer(inner) => inner as &mut dyn Tracing,
			Tracer::OpcodeTracer(inner) => inner as &mut dyn Tracing,
		}
	}

	/// Collect the traces and return them.
	pub fn collect_trace(self) -> Option<Trace> {
		match self {
			Tracer::CallTracer(inner) => inner.collect_trace().map(Trace::Call),
			Tracer::PrestateTracer(inner) => Some(inner.collect_trace().into()),
			Tracer::OpcodeTracer(inner) => Some(inner.collect_trace().into()),
		}
	}

	/// Check if this is an opcode tracer.
	pub fn is_opcode_tracer(&self) -> bool {
		matches!(self, Tracer::OpcodeTracer(_))
	}
}

/// Map DispatchError to Go Ethereum compatible error string
pub fn error_string<T: Config>(error: DispatchError) -> String {
	// Convert our Error enum variants to DispatchError for comparison
	let out_of_gas = Error::<T>::OutOfGas.into();
	let transfer_failed = Error::<T>::TransferFailed.into();
	let max_call_depth_reached = Error::<T>::MaxCallDepthReached.into();
	let out_of_bounds = Error::<T>::OutOfBounds.into();
	let duplicate_contract = Error::<T>::DuplicateContract.into();
	let state_change_denied = Error::<T>::StateChangeDenied.into();
	let contract_reverted = Error::<T>::ContractReverted.into();
	let blob_too_large = Error::<T>::BlobTooLarge.into();
	let invalid_instruction = Error::<T>::InvalidInstruction.into();

	match error {
		err if err == out_of_gas => "out of gas".to_string(),
		err if err == transfer_failed => "insufficient balance for transfer".to_string(),
		err if err == max_call_depth_reached => "max call depth exceeded".to_string(),
		err if err == out_of_bounds => "return data out of bounds".to_string(),
		err if err == duplicate_contract => "contract address collision".to_string(),
		err if err == state_change_denied => "write protection".to_string(),
		err if err == contract_reverted => "execution reverted".to_string(),
		err if err == blob_too_large => "max code size exceeded".to_string(),
		err if err == invalid_instruction => "invalid jump destination".to_string(),
		DispatchError::Module(sp_runtime::ModuleError { message, .. }) =>
			message.unwrap_or_default().to_string(),
		_ => format!("{:?}", error),
	}
}
