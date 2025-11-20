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
	AccountInfo, Code, Config, ExecReturnValue, Key, Pallet, PristineCode,
};
use alloc::{
	collections::{BTreeMap, BTreeSet},
	vec::Vec,
};
use sp_core::{H160, U256};

/// A tracer that traces the prestate.
#[derive(frame_support::DefaultNoBound, Debug, Clone, PartialEq)]
pub struct PrestateTracer<T> {
	/// The tracer configuration.
	config: PrestateTracerConfig,

	/// Stack of calls.
	calls: Vec<H160>,

	/// The code used by create transaction
	create_code: Option<Code>,

	/// List of created contracts addresses.
	created_addrs: BTreeSet<H160>,

	/// List of destructed contracts addresses.
	destructed_addrs: BTreeSet<H160>,

	// pre / post state
	trace: (BTreeMap<H160, PrestateTraceInfo>, BTreeMap<H160, PrestateTraceInfo>),

	_phantom: core::marker::PhantomData<T>,
}

impl<T: Config> PrestateTracer<T>
where
	T::Nonce: Into<u32>,
{
	/// Create a new [`PrestateTracer`] instance.
	pub fn new(config: PrestateTracerConfig) -> Self {
		Self { config, ..Default::default() }
	}

	fn current_addr(&self) -> H160 {
		self.calls.last().copied().unwrap_or_default()
	}

	/// Returns an empty trace.
	pub fn empty_trace(&self) -> PrestateTrace {
		if self.config.diff_mode {
			PrestateTrace::DiffMode { pre: Default::default(), post: Default::default() }
		} else {
			PrestateTrace::Prestate(Default::default())
		}
	}

	/// Collect the traces and return them.
	pub fn collect_trace(self) -> PrestateTrace {
		let (mut pre, mut post) = self.trace;
		let include_code = !self.config.disable_code;

		let is_empty = |info: &PrestateTraceInfo| {
			!info.storage.values().any(|v| v.is_some()) &&
				info.balance.is_none() &&
				info.nonce.is_none() &&
				info.code.is_none()
		};

		if self.config.diff_mode {
			if include_code {
				for addr in &self.created_addrs {
					if let Some(info) = post.get_mut(addr) {
						info.code = Self::bytecode(addr);
					}
				}
			}

			// collect destructed contracts info in post state
			for addr in &self.destructed_addrs {
				Self::update_prestate_info(post.entry(*addr).or_default(), addr, None);
			}

			// clean up the storage that are in pre but not in post these are just read
			pre.iter_mut().for_each(|(addr, info)| {
				if let Some(post_info) = post.get(addr) {
					info.storage.retain(|k, _| post_info.storage.contains_key(k));
				} else {
					info.storage.clear();
				}
			});

			// If the address was created and destructed we do not trace it
			post.retain(|addr, _| {
				if self.created_addrs.contains(addr) && self.destructed_addrs.contains(addr) {
					return false
				}
				true
			});

			pre.retain(|addr, pre_info| {
				if is_empty(&pre_info) {
					return false
				}

				let post_info = post.entry(*addr).or_insert_with_key(|addr| {
					Self::prestate_info(
						addr,
						Pallet::<T>::evm_balance(addr),
						include_code.then(|| Self::bytecode(addr)).flatten(),
					)
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

			post.retain(|_, info| !is_empty(&info));
			PrestateTrace::DiffMode { pre, post }
		} else {
			pre.retain(|_, info| !is_empty(&info));
			PrestateTrace::Prestate(pre)
		}
	}
}

/// Get the appropriate trace entry (pre or post) based on whether
/// the address was created.
/// Returns early if the address is created and diff_mode is false.
macro_rules! get_entry {
	($self:expr, $addr:expr) => {
		if $self.created_addrs.contains(&$addr) {
			if !$self.config.diff_mode {
				return
			}
			$self.trace.1.entry($addr)
		} else {
			$self.trace.0.entry($addr)
		}
	};
}

impl<T: Config> PrestateTracer<T>
where
	T::Nonce: Into<u32>,
{
	/// Get the code of the contract.
	fn bytecode(address: &H160) -> Option<Bytes> {
		let code_hash = AccountInfo::<T>::load_contract(address)?.code_hash;
		let code: Vec<u8> = PristineCode::<T>::get(&code_hash)?.into();
		return Some(code.into())
	}

	/// Update the prestate info for the given address.
	fn update_prestate_info(entry: &mut PrestateTraceInfo, addr: &H160, code: Option<Bytes>) {
		let info = Self::prestate_info(addr, Pallet::<T>::evm_balance(addr), code);
		entry.balance = info.balance;
		entry.nonce = info.nonce;
		entry.code = info.code;
	}

	/// Set the PrestateTraceInfo for the given address.
	fn prestate_info(addr: &H160, balance: U256, code: Option<Bytes>) -> PrestateTraceInfo {
		let mut info = PrestateTraceInfo::default();
		info.balance = Some(balance);
		info.code = code;
		let nonce = Pallet::<T>::evm_nonce(addr);
		info.nonce = if nonce > 0 { Some(nonce) } else { None };
		info
	}

	/// Record a read
	fn read_account(&mut self, addr: H160) {
		let include_code = !self.config.disable_code;
		get_entry!(self, addr).or_insert_with_key(|addr| {
			Self::prestate_info(
				addr,
				Pallet::<T>::evm_balance(addr),
				include_code.then(|| Self::bytecode(addr)).flatten(),
			)
		});
	}
}

impl<T: Config> Tracing for PrestateTracer<T>
where
	T::Nonce: Into<u32>,
{
	fn watch_address(&mut self, addr: &H160) {
		let include_code = !self.config.disable_code;
		self.trace.0.entry(*addr).or_insert_with_key(|addr| {
			Self::prestate_info(
				addr,
				Pallet::<T>::evm_balance(addr),
				include_code.then(|| Self::bytecode(addr)).flatten(),
			)
		});
	}

	fn instantiate_code(&mut self, code: &crate::Code, _salt: Option<&[u8; 32]>) {
		self.create_code = Some(code.clone());
	}

	fn terminate(
		&mut self,
		contract_address: H160,
		beneficiary_address: H160,
		_gas_left: U256,
		_value: U256,
	) {
		self.destructed_addrs.insert(contract_address);
		self.trace.0.entry(beneficiary_address).or_insert_with_key(|addr| {
			Self::prestate_info(addr, Pallet::<T>::evm_balance(addr), None)
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
		_gas_limit: U256,
	) {
		if is_delegate_call {
			self.calls.push(self.current_addr());
		} else {
			self.calls.push(to);
		}

		if self.create_code.take().is_some() {
			self.created_addrs.insert(to);
		}
		self.read_account(from);
		self.read_account(to);
	}

	fn exit_child_span_with_error(&mut self, _error: crate::DispatchError, _gas_used: U256) {
		self.calls.pop();
	}

	fn exit_child_span(&mut self, output: &ExecReturnValue, _gas_used: U256) {
		let current_addr = self.calls.pop().unwrap_or_default();
		if output.did_revert() {
			return
		}

		let code = if self.config.disable_code { None } else { Self::bytecode(&current_addr) };

		Self::update_prestate_info(
			self.trace.1.entry(current_addr).or_default(),
			&current_addr,
			code,
		);
	}

	fn storage_write(&mut self, key: &Key, old_value: Option<Vec<u8>>, new_value: Option<&[u8]>) {
		let current_addr = self.current_addr();
		let key = Bytes::from(key.unhashed().to_vec());

		let old_value = get_entry!(self, current_addr)
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
				.entry(current_addr)
				.or_default()
				.storage
				.insert(key, new_value.map(|v| v.to_vec().into()));
		} else {
			self.trace.1.entry(self.current_addr()).or_default().storage.remove(&key);
		}
	}

	fn storage_read(&mut self, key: &Key, value: Option<&[u8]>) {
		let current_addr = self.current_addr();

		get_entry!(self, current_addr)
			.or_default()
			.storage
			.entry(key.unhashed().to_vec().into())
			.or_insert_with(|| value.map(|v| v.to_vec().into()));
	}

	fn balance_read(&mut self, addr: &H160, value: U256) {
		let include_code = !self.config.disable_code;
		get_entry!(self, *addr).or_insert_with_key(|addr| {
			Self::prestate_info(addr, value, include_code.then(|| Self::bytecode(addr)).flatten())
		});
	}
}
