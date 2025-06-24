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
	evm::{Bytes, PrestateTrace, PrestateTraceInfo, PrestateTracerConfig},
	tracing::Tracing,
	BalanceOf, Bounded, Config, ContractInfoOf, ExecReturnValue, Key, MomentOf, Pallet,
	PristineCode, Weight,
};
use alloc::{collections::BTreeMap, vec::Vec};
use sp_core::{H160, H256, U256};

/// A tracer that traces the prestate.
#[derive(frame_support::DefaultNoBound, Debug, Clone, PartialEq)]
pub struct PrestateTracer<T> {
	/// The tracer configuration.
	config: PrestateTracerConfig,

	/// The current address of the contract's which storage is being accessed.
	current_addr: H160,

	// pre / post state
	trace: (BTreeMap<H160, PrestateTraceInfo>, BTreeMap<H160, PrestateTraceInfo>),

	_phantom: core::marker::PhantomData<T>,
}

impl<T: Config> PrestateTracer<T>
where
	BalanceOf<T>: Into<U256> + TryFrom<U256> + Bounded,
	MomentOf<T>: Into<U256>,
	T::Hash: frame_support::traits::IsType<H256>,
	T::Nonce: Into<u32>,
{
	/// Create a new [`PrestateTracer`] instance.
	pub fn new(config: PrestateTracerConfig) -> Self {
		Self { config, ..Default::default() }
	}

	/// Returns an empty trace.
	pub fn empty_trace(&self) -> PrestateTrace {
		if self.config.diff_mode {
			PrestateTrace::Prestate(Default::default())
		} else {
			PrestateTrace::DiffMode { pre: Default::default(), post: Default::default() }
		}
	}

	/// Collect the traces and return them.
	pub fn collect_trace(&mut self) -> PrestateTrace {
		let trace = core::mem::take(&mut self.trace);
		let (mut pre, mut post) = trace;
		let disable_code = self.config.disable_code;
		if self.config.diff_mode {
			// clean up the storage that are in pre but not in post these are just read
			pre.iter_mut().for_each(|(addr, info)| {
				if let Some(post_info) = post.get(addr) {
					info.storage.retain(|k, _| post_info.storage.contains_key(k));
				} else {
					info.storage.clear();
				}
			});

			pre.retain(|addr, pre_info| {
				let post_info = post.entry(*addr).or_insert_with_key(|addr| {
					Self::prestate_info(addr, Pallet::<T>::evm_balance(addr), disable_code)
				});

				if post_info == pre_info {
					post.remove(addr);
					return false
				}

				if post_info.code == pre_info.code {
					post_info.code = None;
				}

				if post_info.balance == pre_info.balance {
					post_info.balance = None;
				}

				if post_info.nonce == pre_info.nonce {
					post_info.nonce = None;
				}

				if post_info == &Default::default() {
					post.remove(addr);
				}

				true
			});

			PrestateTrace::DiffMode { pre, post }
		} else {
			PrestateTrace::Prestate(pre)
		}
	}
}

impl<T: Config> PrestateTracer<T>
where
	BalanceOf<T>: Into<U256> + TryFrom<U256> + Bounded,
	MomentOf<T>: Into<U256>,
	T::Hash: frame_support::traits::IsType<H256>,
	T::Nonce: Into<u32>,
{
	/// Get the code of the contract.
	fn bytecode(address: &H160) -> Option<Bytes> {
		let code_hash = ContractInfoOf::<T>::get(address)?.code_hash;
		let code: Vec<u8> = PristineCode::<T>::get(&code_hash)?.into();
		return Some(code.into())
	}

	/// Update the prestate info for the given address.
	fn update_prestate_info(entry: &mut PrestateTraceInfo, addr: &H160, disable_code: bool) {
		let info = Self::prestate_info(addr, Pallet::<T>::evm_balance(addr), disable_code);
		entry.balance = info.balance;
		entry.nonce = info.nonce;
		entry.code = info.code;
	}

	/// Set the PrestateTraceInfo for the given address.
	fn prestate_info(addr: &H160, balance: U256, disable_code: bool) -> PrestateTraceInfo {
		let mut info = PrestateTraceInfo::default();
		info.balance = Some(balance);
		let nonce = Pallet::<T>::evm_nonce(addr);
		info.nonce = if nonce > 0 { Some(nonce) } else { None };
		if !disable_code {
			info.code = Self::bytecode(addr);
		}
		info
	}
}

impl<T: Config> Tracing for PrestateTracer<T>
where
	BalanceOf<T>: Into<U256> + TryFrom<U256> + Bounded,
	MomentOf<T>: Into<U256>,
	T::Hash: frame_support::traits::IsType<H256>,
	T::Nonce: Into<u32>,
{
	fn watch_address(&mut self, addr: &H160) {
		let disable_code = self.config.disable_code;
		self.trace.0.entry(*addr).or_insert_with_key(|addr| {
			Self::prestate_info(addr, Pallet::<T>::evm_balance(addr), disable_code)
		});
	}

	fn enter_child_span(
		&mut self,
		from: H160,
		to: H160,
		is_delegate_call: bool,
		_is_read_only: bool,
		_value: U256,
		_input: &[u8],
		_gas: Weight,
	) {
		let disable_code = self.config.disable_code;
		self.trace.0.entry(from).or_insert_with_key(|addr| {
			Self::prestate_info(addr, Pallet::<T>::evm_balance(addr), disable_code)
		});

		if to != H160::zero() {
			self.trace.0.entry(to).or_insert_with_key(|addr| {
				Self::prestate_info(addr, Pallet::<T>::evm_balance(addr), disable_code)
			});
		}

		if !is_delegate_call {
			self.current_addr = to;
		}
	}

	fn exit_child_span(&mut self, output: &ExecReturnValue, _gas_used: Weight) {
		if output.did_revert() {
			return
		}

		Self::update_prestate_info(
			self.trace.1.entry(self.current_addr).or_default(),
			&self.current_addr,
			self.config.disable_code,
		);
	}

	fn storage_write(&mut self, key: &Key, old_value: Option<Vec<u8>>, new_value: Option<&[u8]>) {
		let key = Bytes::from(key.unhashed().to_vec());

		let old_value = self
			.trace
			.0
			.entry(self.current_addr)
			.or_default()
			.storage
			.entry(key.clone())
			.or_insert_with(|| old_value.map(Into::into));

		if !self.config.diff_mode {
			return
		}

		if old_value.as_ref().map(|v| v.0.as_ref()) != new_value {
			self.trace
				.1
				.entry(self.current_addr)
				.or_default()
				.storage
				.insert(key, new_value.map(|v| v.to_vec().into()));
		} else {
			self.trace.1.entry(self.current_addr).or_default().storage.remove(&key);
		}
	}

	fn storage_read(&mut self, key: &Key, value: Option<&[u8]>) {
		self.trace
			.0
			.entry(self.current_addr)
			.or_default()
			.storage
			.entry(key.unhashed().to_vec().into())
			.or_insert_with(|| value.map(|v| v.to_vec().into()));
	}

	fn balance_read(&mut self, addr: &H160, value: U256) {
		self.trace
			.0
			.entry(*addr)
			.or_insert_with_key(|addr| Self::prestate_info(addr, value, self.config.disable_code));
	}
}
