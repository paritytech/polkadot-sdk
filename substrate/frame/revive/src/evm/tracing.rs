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
	evm::{CallTrace, Trace},
	tracing::Tracing,
	BalanceOf, Bounded, Config, MomentOf, Weight,
};
use sp_core::{H256, U256};

mod call_tracing;
pub use call_tracing::*;

mod prestate_tracing;
pub use prestate_tracing::*;

/// A composite tracer.
#[derive(derive_more::From, Debug)]
pub enum Tracer<T> {
	/// A tracer that traces calls.
	CallTracer(CallTracer<U256, fn(Weight) -> U256>),
	/// A tracer that traces the prestate.
	PrestateTracer(PrestateTracer<T>),
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
		}
	}

	/// Get a mutable trait‐object reference to the inner tracer.
	pub fn as_tracing(&mut self) -> &mut (dyn Tracing + 'static) {
		match self {
			Tracer::CallTracer(inner) => inner as &mut dyn Tracing,
			Tracer::PrestateTracer(inner) => inner as &mut dyn Tracing,
		}
	}

	/// Collect the traces and return them.
	pub fn collect_trace(&mut self) -> Option<Trace> {
		match self {
			Tracer::CallTracer(inner) => inner.collect_trace().map(Trace::Call),
			Tracer::PrestateTracer(inner) => Some(inner.collect_trace().into()),
		}
	}
}
