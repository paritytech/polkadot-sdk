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

/// The identity precompile.
pub struct Identity;

impl<T: Config> Precompile<T> for Identity {
	fn execute(gas_meter: &mut GasMeter<T>, input: &[u8]) -> Result<ExecReturnValue, &'static str> {
		gas_meter.charge(RuntimeCosts::Identity(input.len() as u32))?;

		Ok(ExecReturnValue { data: input.to_vec(), flags: ReturnFlags::empty() })
	}
}
