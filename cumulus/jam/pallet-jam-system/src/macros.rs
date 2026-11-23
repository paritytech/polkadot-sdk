// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! The macro exporting the JAM validation entry point.

/// Export `jam_validate_block`, the entry point the Parachain Service's refine calls (spec §4.2):
/// zero arguments, inputs and outputs through the child-PVM host calls.
///
/// Expects the runtime and its block executor:
///
/// ```ignore
/// cumulus_pallet_jam_system::register_jam_validate_block! {
/// 	Runtime = Runtime,
/// 	BlockExecutor = Executive,
/// }
/// ```
#[macro_export]
macro_rules! register_jam_validate_block {
	(Runtime = $runtime:ty, BlockExecutor = $block_executor:ty $(,)?) => {
		#[doc(hidden)]
		mod jam_parachain_validate_block {
			use super::*;

			#[$crate::__private::polkavm_export(abi = $crate::__private::polkavm_abi)]
			unsafe fn jam_validate_block() {
				$crate::__private::jam_validate_block::<
					<$runtime as $crate::__private::GetRuntimeBlockType>::RuntimeBlock,
					$block_executor,
					$runtime,
				>()
			}
		}
	};
}
