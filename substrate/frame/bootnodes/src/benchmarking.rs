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

//! Benchmarks for `pallet_bootnodes`.

use super::*;
use alloc::vec;
use frame_benchmarking::v2::*;
use sp_core::crypto::KeyTypeId;

/// Produces node keys and proof-of-possession signatures for benchmarks.
pub trait BenchmarkHelper {
	/// Return a fresh ed25519 public key and its signature over `payload`.
	fn sign(payload: &[u8]) -> (ed25519::Public, ed25519::Signature);
}

/// Key type under which the default helper keeps benchmark keys in the keystore.
const BENCH_KEY_TYPE: KeyTypeId = KeyTypeId(*b"boot");

/// Default helper, signing through the keystore the benchmarking host provides.
impl BenchmarkHelper for () {
	fn sign(payload: &[u8]) -> (ed25519::Public, ed25519::Signature) {
		let public = sp_io::crypto::ed25519_generate(BENCH_KEY_TYPE, None);
		let sig = sp_io::crypto::ed25519_sign(BENCH_KEY_TYPE, &public, payload)
			.expect("the key was just generated in the keystore; qed");
		(public, sig)
	}
}

fn encode_varint(mut value: usize, out: &mut Vec<u8>) {
	while value >= 0x80 {
		out.push((value as u8 & 0x7f) | 0x80);
		value >>= 7;
	}
	out.push(value as u8);
}

/// The largest `/dns/<label>/tcp/30333` multiaddr of at most `max_len` bytes.
fn max_multiaddr(max_len: u32) -> Vec<u8> {
	let max_len = max_len as usize;
	let mut label_len = max_len.saturating_sub(5);
	loop {
		let mut addr = vec![53];
		encode_varint(label_len, &mut addr);
		if addr.len() + label_len + 3 <= max_len || label_len == 0 {
			addr.extend(core::iter::repeat_n(b'a', label_len));
			addr.extend([6, 0x76, 0x7d]);
			return addr;
		}
		label_len -= 1;
	}
}

/// A bootnode with a fresh key and a valid proof of possession over `seq` and `addr`.
fn bootnode<T: Config>(seq: u64, addr: Vec<u8>) -> Result<BootnodeOf<T>, BenchmarkError> {
	let addr: BoundedVec<u8, T::MaxAddrLen> =
		addr.try_into().map_err(|_| BenchmarkError::Stop("MaxAddrLen is too small"))?;
	let payload = Pallet::<T>::pop_payload(seq, &addr);
	let (node_key, sig) = T::BenchmarkHelper::sign(&payload);
	Ok(Bootnode { addr, node_key, seq, sig })
}

/// Fill the set with `n` bootnodes carrying maximum-length addresses.
fn fill<T: Config>(n: u32) -> Result<(), BenchmarkError> {
	for _ in 0..n {
		let bootnode = bootnode::<T>(0, max_multiaddr(T::MaxAddrLen::get()))?;
		Pallet::<T>::do_add_bootnode(bootnode)?;
	}
	Ok(())
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn add_bootnode() -> Result<(), BenchmarkError> {
		// Worst case: the set is one short of full and the new node key already has a sequence
		// number on record, so both storage items are read and written.
		fill::<T>(T::MaxBootnodes::get().saturating_sub(1))?;
		let bootnode = bootnode::<T>(1, max_multiaddr(T::MaxAddrLen::get()))?;
		LastSeq::<T>::insert(bootnode.node_key, 0);
		let origin =
			T::AdminOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, bootnode.clone());

		assert!(Bootnodes::<T>::get().iter().any(|b| b == &bootnode));
		assert_eq!(LastSeq::<T>::get(bootnode.node_key), Some(1));
		Ok(())
	}

	#[benchmark]
	fn remove_bootnode() -> Result<(), BenchmarkError> {
		// Worst case: a full set, removing the last entry.
		fill::<T>(T::MaxBootnodes::get())?;
		let node_key = Bootnodes::<T>::get()
			.last()
			.map(|b| b.node_key)
			.ok_or(BenchmarkError::Stop("MaxBootnodes is zero"))?;
		let origin =
			T::AdminOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, node_key);

		assert!(!Bootnodes::<T>::get().iter().any(|b| b.node_key == node_key));
		assert_eq!(LastSeq::<T>::get(node_key), Some(0));
		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn max_multiaddr_is_valid_and_within_one_byte_of_the_bound_for_every_prefix_width() {
		for max_len in (5..=140).chain(16_380..=16_400).chain([100_000]) {
			let addr = max_multiaddr(max_len);
			assert_eq!(multiaddr::validate(&addr), Ok(()), "max_len {max_len}");
			assert!(addr.len() <= max_len as usize, "max_len {max_len}: {} bytes", addr.len());
			assert!(addr.len() + 1 >= max_len as usize, "max_len {max_len}: {} bytes", addr.len());
		}
	}
}
