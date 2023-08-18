// Copyright 2023 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

pub use bridge_hub_kusama_runtime::{
	constants::fee::WeightToFee, xcm_config::XcmConfig, AllPalletsWithoutSystem, Balances,
	ExistentialDeposit, ParachainSystem, PolkadotXcm, Runtime, RuntimeEvent, SessionKeys,
};
use codec::Decode;
use frame_support::parameter_types;
use parachains_common::{AccountId, AuraId};

const ALICE: [u8; 32] = [1u8; 32];

parameter_types! {
	pub CheckingAccount: AccountId = PolkadotXcm::check_account();
}

bridge_hub_test_utils::test_cases::include_teleports_for_native_asset_works!(
	Runtime,
	AllPalletsWithoutSystem,
	XcmConfig,
	CheckingAccount,
	WeightToFee,
	ParachainSystem,
	bridge_hub_test_utils::CollatorSessionKeys::new(
		AccountId::from(ALICE),
		AccountId::from(ALICE),
		SessionKeys { aura: AuraId::from(sp_core::sr25519::Public::from_raw(ALICE)) }
	),
	ExistentialDeposit::get(),
	Box::new(|runtime_event_encoded: Vec<u8>| {
		match RuntimeEvent::decode(&mut &runtime_event_encoded[..]) {
			Ok(RuntimeEvent::PolkadotXcm(event)) => Some(event),
			_ => None,
		}
	}),
	Box::new(|runtime_event_encoded: Vec<u8>| {
		match RuntimeEvent::decode(&mut &runtime_event_encoded[..]) {
			Ok(RuntimeEvent::XcmpQueue(event)) => Some(event),
			_ => None,
		}
	}),
	1002
);
