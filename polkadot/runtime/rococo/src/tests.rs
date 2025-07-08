// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot. If not, see <http://www.gnu.org/licenses/>.

//! Tests for the Rococo Runtime Configuration

use crate::*;
use std::collections::HashSet;

use crate::xcm_config::LocationConverter;
use frame_support::traits::WhitelistedStorageKeys;
use sp_core::{crypto::Ss58Codec, hexdisplay::HexDisplay};
use sp_keyring::Sr25519Keyring::Alice;
use xcm_runtime_apis::conversions::LocationToAccountHelper;

#[test]
fn check_whitelist() {
	let whitelist: HashSet<String> = AllPalletsWithSystem::whitelisted_storage_keys()
		.iter()
		.map(|e| HexDisplay::from(&e.key).to_string())
		.collect();

	// Block number
	assert!(whitelist.contains("26aa394eea5630e07c48ae0c9558cef702a5c1b19ab7a04f536c519aca4983ac"));
	// Total issuance
	assert!(whitelist.contains("c2261276cc9d1f8598ea4b6a74b15c2f57c875e4cff74148e4628f264b974c80"));
	// Execution phase
	assert!(whitelist.contains("26aa394eea5630e07c48ae0c9558cef7ff553b5a9862a516939d82b3d3d8661a"));
	// Event count
	assert!(whitelist.contains("26aa394eea5630e07c48ae0c9558cef70a98fdbe9ce6c55837576c60c7af3850"));
	// System events
	assert!(whitelist.contains("26aa394eea5630e07c48ae0c9558cef780d41e5e16056765bc8461851072c9d7"));
	// XcmPallet VersionDiscoveryQueue
	assert!(whitelist.contains("1405f2411d0af5a7ff397e7c9dc68d194a222ba0333561192e474c59ed8e30e1"));
	// XcmPallet SafeXcmVersion
	assert!(whitelist.contains("1405f2411d0af5a7ff397e7c9dc68d196323ae84c43568be0d1394d5d0d522c4"));
}

#[test]
fn check_treasury_pallet_id() {
	assert_eq!(
		<Treasury as frame_support::traits::PalletInfoAccess>::index() as u8,
		rococo_runtime_constants::TREASURY_PALLET_ID
	);
}

mod encoding_tests {
	use super::*;

	#[test]
	fn nis_hold_reason_encoding_is_correct() {
		assert_eq!(RuntimeHoldReason::Nis(pallet_nis::HoldReason::NftReceipt).encode(), [38, 0]);
	}
}

#[test]
fn location_conversion_works() {
	// the purpose of hardcoded values is to catch an unintended location conversion logic change.
	struct TestCase {
		description: &'static str,
		location: Location,
		expected_account_id_str: &'static str,
	}

	let test_cases = vec![
		// DescribeTerminus
		TestCase {
			description: "DescribeTerminus Child",
			location: Location::new(0, [Parachain(1111)]),
			expected_account_id_str: "5Ec4AhP4h37t7TFsAZ4HhFq6k92usAAJDUC3ADSZ4H4Acru3",
		},
		// DescribePalletTerminal
		TestCase {
			description: "DescribePalletTerminal Child",
			location: Location::new(0, [Parachain(1111), PalletInstance(50)]),
			expected_account_id_str: "5FjEBrKn3STAFsZpQF4jzwxUYHNGnNgzdZqSQfTzeJ82XKp6",
		},
		// DescribeAccountId32Terminal
		TestCase {
			description: "DescribeAccountId32Terminal Child",
			location: Location::new(
				0,
				[Parachain(1111), AccountId32 { network: None, id: AccountId::from(Alice).into() }],
			),
			expected_account_id_str: "5EEMro9RRDpne4jn9TuD7cTB6Amv1raVZ3xspSkqb2BF3FJH",
		},
		// DescribeAccountKey20Terminal
		TestCase {
			description: "DescribeAccountKey20Terminal Child",
			location: Location::new(
				0,
				[Parachain(1111), AccountKey20 { network: None, key: [0u8; 20] }],
			),
			expected_account_id_str: "5HohjXdjs6afcYcgHHSstkrtGfxgfGKsnZ1jtewBpFiGu4DL",
		},
		// DescribeTreasuryVoiceTerminal
		TestCase {
			description: "DescribeTreasuryVoiceTerminal Child",
			location: Location::new(
				0,
				[Parachain(1111), Plurality { id: BodyId::Treasury, part: BodyPart::Voice }],
			),
			expected_account_id_str: "5GenE4vJgHvwYVcD6b4nBvH5HNY4pzpVHWoqwFpNMFT7a2oX",
		},
		// DescribeBodyTerminal
		TestCase {
			description: "DescribeBodyTerminal Child",
			location: Location::new(
				0,
				[Parachain(1111), Plurality { id: BodyId::Unit, part: BodyPart::Voice }],
			),
			expected_account_id_str: "5DPgGBFTTYm1dGbtB1VWHJ3T3ScvdrskGGx6vSJZNP1WNStV",
		},
	];

	for tc in test_cases {
		let expected =
			AccountId::from_string(tc.expected_account_id_str).expect("Invalid AccountId string");

		let got = LocationToAccountHelper::<AccountId, LocationConverter>::convert_location(
			tc.location.into(),
		)
		.unwrap();

		assert_eq!(got, expected, "{}", tc.description);
	}
}
