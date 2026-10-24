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

//! The relay chain's HRMP channel deposits, held here by `pallet-hrmp-para`.

use crate::*;
use codec::Encode;
use hrmp_primitives::MessageToRelay;
use polkadot_parachain_primitives::primitives::Sibling;
use sp_runtime::traits::{AccountIdConversion, Convert};
use westend_runtime_constants::hrmp_deposits::RELAY_PALLET_INDEX;
use xcm::latest::prelude::*;

/// Sends `pallet-hrmp-para`'s answers to `pallet-hrmp-relay` on the relay chain, as this chain.
pub struct HrmpToRelay;
impl pallet_hrmp_para::SendToRelay for HrmpToRelay {
	fn send(message: MessageToRelay) -> Result<(), ()> {
		// `HrmpRelay::receive` on the relay chain.
		let call = (RELAY_PALLET_INDEX, pallet_hrmp_relay::RECEIVE_CALL_INDEX, message).encode();
		let message = Xcm(vec![
			Instruction::UnpaidExecution {
				weight_limit: WeightLimit::Unlimited,
				check_origin: None,
			},
			Instruction::Transact {
				origin_kind: OriginKind::Native,
				fallback_max_weight: None,
				call: call.into(),
			},
		]);
		PolkadotXcm::send_xcm(Here, Location::parent(), message)
			.map(|_| ())
			.map_err(|e| {
				tracing::error!(target: "runtime::hrmp-para", error=?e, "sending to relay failed");
			})
	}
}

/// A para's sovereign account on this chain, the one `LocationToAccountId` gives
/// `(1, [Parachain(id)])`.
pub struct SiblingSovereignAccount;
impl Convert<u32, AccountId> for SiblingSovereignAccount {
	fn convert(id: u32) -> AccountId {
		Sibling::from(id).into_account_truncating()
	}
}

impl pallet_hrmp_para::Config for Runtime {
	type Currency = Balances;
	type SendToRelay = HrmpToRelay;
	type SovereignAccountOf = SiblingSovereignAccount;
	type WeightInfo = pallet_hrmp_para::weights::SubstrateWeight<Runtime>;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = HrmpBenchmarkHelper;
}

/// Makes the relay chain reachable for the benchmarks of `pallet-hrmp-para`.
#[cfg(feature = "runtime-benchmarks")]
pub struct HrmpBenchmarkHelper;

#[cfg(feature = "runtime-benchmarks")]
impl pallet_hrmp_para::BenchmarkHelper for HrmpBenchmarkHelper {
	fn ensure_send_to_relay() {
		use cumulus_primitives_core::UpwardMessageSender;
		<ParachainSystem as UpwardMessageSender>::ensure_successful_delivery();
	}
}
