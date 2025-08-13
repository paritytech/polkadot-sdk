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

//! Various predefined implementations supporting header storage on AssetHub


pub struct StoreParaHeadersFor<T, I, Chain, Sender, Message>(
	core::marker::PhantomData<(T, I, Chain, Sender, Message)>,
);

#[cfg(feature = "runtime-benchmarks")]
impl<
		T: pallet_bridge_proof_root_store::Config<I, Key = ParaHash, Value = ParaHash>,
		I: 'static,
		Chain: Parachain<Hash = ParaHash>,
		Sender,
		Message,
	> pallet_bridge_proof_root_store::BenchmarkHelper<ParaHash, ParaHash>
	for StoreParaHeadersFor<T, I, Chain, Sender, Message>
{
	fn create_key_value_for(id: u32) -> (ParaHash, ParaHash) {
		let para_header_number = id;
		let mut para_hash = [0_u8; 32];
		para_hash[..4].copy_from_slice(&para_header_number.to_le_bytes());
		let para_state_root = ParaHash::from(para_hash);

		(ParaHash::from(para_hash), para_state_root)
	}
}
