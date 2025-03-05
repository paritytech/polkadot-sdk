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
use super::Precompile;
use crate::{Config, ExecReturnValue, GasMeter, RuntimeCosts};
use pallet_revive_uapi::ReturnFlags;

/// The Sha256 precompile.
pub struct Sha256;

impl<T: Config> Precompile<T> for Sha256 {
	fn execute(gas_meter: &mut GasMeter<T>, input: &[u8]) -> Result<ExecReturnValue, &'static str> {
		gas_meter.charge(RuntimeCosts::HashSha256(input.len() as u32))?;
		let data = sp_io::hashing::sha2_256(input).to_vec();
		Ok(ExecReturnValue { data, flags: ReturnFlags::empty() })
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::pure_precompiles::test::test_precompile_test_vectors;

	#[test]
	fn test_sha256() -> Result<(), String> {
		test_precompile_test_vectors::<Sha256>(include_str!("./testdata/2-sha256.json"))?;
		Ok(())
	}
}
