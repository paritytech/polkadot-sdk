// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
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
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use super::{
	asset_para,
	network::{
		AssetPara, MockNet, Relay, SimplePara, ALICE, ASSET_PARA_ID, BOB, CENTS, INITIAL_BALANCE,
		SIMPLE_PARA_ID, UNITS,
	},
	relay_chain, simple_para,
};
use frame::{prelude::fungible::Mutate, testing_prelude::*};
use test_log::test;
use xcm::prelude::*;
use xcm_executor::traits::ConvertLocation;
use xcm_simulator::TestExt;

#[docify::export]
#[test]
fn registering_foreign_assets_work() {
	// We restart the mock network.
	MockNet::reset();

	// ALICE starts with INITIAL_BALANCE on the relay chain
	Relay::execute_with(|| {
		assert_eq!(relay_chain::Balances::free_balance(&ALICE), INITIAL_BALANCE);
	});

	// BOB starts with 0 on the parachain
	SimplePara::execute_with(|| {
		assert_eq!(simple_para::Balances::free_balance(&BOB), 0);
	});

	let simple_para_sovereign = asset_para::xcm_config::LocationToAccountId::convert_location(
		&Location::new(1, Parachain(SIMPLE_PARA_ID)),
	)
	.expect("Can convert");

	AssetPara::execute_with(|| {
		assert_ok!(asset_para::Balances::mint_into(&simple_para_sovereign, 10 * UNITS));
		assert_eq!(asset_para::Balances::free_balance(&simple_para_sovereign), 10 * UNITS);
	});

	SimplePara::execute_with(|| {
		// We specify `Parent` because we are referencing the Relay Chain token.
		// This chain doesn't have a token of its own, so we always refer to this token,
		// and we do so by the Location of the Relay Chain.
		let fee_payment: Asset = (Location::here(), 10u128 * UNITS).into();

		let xcm = Xcm(vec![
			// SetAppendix(Xcm(vec![
			// 	RefundSurplus,
			// 	DepositAsset {
			// 		assets: AssetFilter::Wild(WildAsset::All),
			// 		beneficiary: Location::new(1, Parachain(SIMPLE_PARA_ID)),
			// 	},
			// ])),
			// WithdrawAsset(fee_payment.clone().into()),
			// PayFees { asset: fee_payment },
			Transact {
				origin_kind: OriginKind::Xcm,
				fallback_max_weight: None,
				call: asset_para::RuntimeCall::ForeignAssets(pallet_assets::Call::create {
					id: Location::new(1, Parachain(SIMPLE_PARA_ID)),
					admin: simple_para_sovereign.into(),
					min_balance: 1_000_000_000,
				})
					.encode()
					.into(),
			},
			Transact {
				origin_kind: OriginKind::SovereignAccount,
				fallback_max_weight: None,
				call: asset_para::RuntimeCall::ForeignAssets(pallet_assets::Call::set_metadata {
					id: Location::new(1, Parachain(SIMPLE_PARA_ID)),
					name: "Simple Para Token".into(),
					symbol: "TOK".into(),
					decimals: 12,
				})
				.encode()
				.into(),
			},
		]);

		assert_ok!(simple_para::XcmPallet::send(
			simple_para::RuntimeOrigin::root(),
			Box::new(Location::new(1, Parachain(ASSET_PARA_ID)).into()),
			Box::new(VersionedXcm::V5(xcm)),
		));
	});

	AssetPara::execute_with(|| {
		asset_para::System::assert_has_event(asset_para::RuntimeEvent::ForeignAssets(
			pallet_assets::Event::MetadataSet {
				asset_id: Location::new(1, Parachain(SIMPLE_PARA_ID)),
				name: "Simple Para Token".into(),
				symbol: "TOK".into(),
				decimals: 12,
				is_frozen: false,
			},
		))
	});
}
