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
//! JSON-RPC methods and types, for Ethereum.

mod byte;
pub use byte::*;

mod rlp_codec;
pub use rlp;

mod type_id;
pub use type_id::*;

mod rpc_types;
mod rpc_types_gen;
pub use rpc_types_gen::*;

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

#[cfg(feature = "std")]
mod account;

#[cfg(feature = "std")]
pub use account::*;

mod signature;
use rlp::{Decodable, DecoderError, Encodable};

/// A type used to encode the `input` field of an Ethereum transaction
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EthInstantiateInput {
	/// The bytecode of the contract.
	pub code: Vec<u8>,
	/// The data to pass to the constructor.
	pub data: Vec<u8>,
}

impl Encodable for EthInstantiateInput {
	fn rlp_append(&self, stream: &mut rlp::RlpStream) {
		stream.begin_list(2usize);
		stream.append(&self.code);
		stream.append(&self.data);
	}
}

impl Decodable for EthInstantiateInput {
	fn decode(rlp: &rlp::Rlp) -> Result<Self, DecoderError> {
		let result = EthInstantiateInput { code: rlp.val_at(0)?, data: rlp.val_at(1)? };
		Ok(result)
	}
}

#[test]
fn eth_instantiate_rlp_codec_works() {
	let input = EthInstantiateInput { code: vec![1, 2, 3], data: vec![4, 5, 6] };
	let encoded = rlp::encode(&input);
	let decoded = rlp::decode::<EthInstantiateInput>(&encoded).unwrap();
	assert_eq!(input, decoded);
}
