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

use crate::{exec::ExecResult,  Config, Error, ExecReturnValue, GasMeter, H160, LOG_TARGET};

mod ecrecover;
pub use ecrecover::*;

mod sha256;
pub use sha256::*;

mod ripemd160;
pub use ripemd160::*;

mod identity;
pub use identity::*;

mod bn128;
pub use bn128::*;

mod modexp;
pub use modexp::*;

mod blake2f;
pub use blake2f::*;

/// Determine if the given address is a precompile.
/// For now, we consider that all addresses between 0x1 and 0xff are reserved for precompiles.
pub fn is_precompile(address: &H160) -> bool {
	let bytes = address.as_bytes();
	bytes.starts_with(&[0u8; 19]) && bytes[19] != 0
}

/// The `Precompile` trait defines the functionality for executing a precompiled contract.
pub trait Precompile<T: Config> {
	/// Executes the precompile with the provided input data.
	fn execute(gas_meter: &mut GasMeter<T>, input: &[u8]) -> Result<ExecReturnValue, &'static str>;
}

pub struct Precompiles<T: Config> {
	_phantom: core::marker::PhantomData<T>,
}


impl<T: Config> Precompiles<T> {
	pub fn execute(addr: H160, gas_meter: &mut GasMeter<T>, input: &[u8]) -> ExecResult {
		match addr.as_bytes()[19] {
			1u8 => ECRecover::execute(gas_meter, input),
			2u8 => Sha256::execute(gas_meter, input),
			3u8 => Ripemd160::execute(gas_meter, input),
			4u8 => Identity::execute(gas_meter, input),
			5u8 => Modexp::execute(gas_meter, input),
			6u8 => Bn128Add::execute(gas_meter, input),
			7u8 => Bn128Mul::execute(gas_meter, input),
			8u8 => Bn128Pairing::execute(gas_meter, input),
			9u8 => Blake2F::execute(gas_meter, input),
			_ => return Err(Error::<T>::UnsupportedPrecompileAddress.into()),
		}
		.map_err(|reason| {
			log::debug!(target: LOG_TARGET, "Precompile failed: {reason:?}");
			Error::<T>::PrecompileFailure.into()
		})
	}
	
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::{tests::Test, ExecReturnValue, Weight};
	use alloy_core::hex;
	use pallet_revive_uapi::ReturnFlags;

	#[derive(Debug, serde::Deserialize)]
	#[serde(rename_all = "PascalCase")]
	struct EthConsensusTest {
		input: String,
		expected: String,
		name: String,
	}

	#[derive(Debug, serde::Deserialize)]
	#[serde(rename_all = "PascalCase")]
	struct EthConsensusFailureTest {
		input: String,
		expected_error: String,
		name: String,
	}

	/// Run a precompile with the given input data.
	pub fn run_precompile<P: Precompile<crate::tests::Test>>(
		input: Vec<u8>,
	) -> Result<ExecReturnValue, &'static str> {
		let mut gas_meter = GasMeter::<Test>::new(Weight::MAX);
		P::execute(&mut gas_meter, &input)
	}

	/// Tests a precompile against the ethereum consensus tests defined in the given json
	/// The  JSON format is expected to contain an array of test vectors,
	/// where each vector can be deserialized into an "EthConsensusTest".
	pub fn test_precompile_test_vectors<P: Precompile<crate::tests::Test>>(
		json: &str,
	) -> Result<(), String> {
		let tests: Vec<EthConsensusTest> = serde_json::from_str(json).expect("expected json array");

		for test in tests {
			let input: Vec<u8> =
				hex::decode(test.input).expect("Could not hex-decode test input data");

			let mut gas_meter = GasMeter::<Test>::new(Weight::MAX);
			match P::execute(&mut gas_meter, &input) {
				Ok(ExecReturnValue { data, flags }) => {
					assert_eq!(
						flags,
						ReturnFlags::empty(),
						"test '{}' failed (unexpected flags)",
						test.name
					);

					assert_eq!(
						hex::encode(data),
						test.expected,
						"test '{}' failed (different output)",
						test.name
					);
				},
				Err(err) => {
					return Err(format!("Test '{}' returned error: {:?}", test.name, err));
				},
			}
		}

		Ok(())
	}

	pub fn test_precompile_failure_test_vectors<P: Precompile<crate::tests::Test>>(
		json: &str,
	) -> Result<(), String> {
		let tests: Vec<EthConsensusFailureTest> =
			serde_json::from_str(json).expect("expected json array");

		for test in tests {
			let input: Vec<u8> =
				hex::decode(test.input).expect("Could not hex-decode test input data");

			let mut gas_meter = GasMeter::<Test>::new(Weight::MAX);
			match P::execute(&mut gas_meter, &input) {
				Ok(ExecReturnValue { data, .. }) => {
					panic!("Test should failed, got {data:?}");
				},
				Err(reason) => {
					assert_eq!(
						test.expected_error, reason,
						"Test '{}' failed (different error)",
						test.name
					);
				},
			}
		}

		Ok(())
	}
}
