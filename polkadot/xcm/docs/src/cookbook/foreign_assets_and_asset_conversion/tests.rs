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

use super::{asset_para, network::{AssetPara, MockNet, Relay, SimplePara, ALICE, BOB, CENTS, INITIAL_BALANCE}, relay_chain, simple_para};
use frame::testing_prelude::*;
use test_log::test;
use xcm::prelude::*;
use xcm_simulator::TestExt;
use crate::cookbook::foreign_assets_and_asset_conversion::network::{ASSET_PARA_ID, SIMPLE_PARA_ID};

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


	SimplePara::execute_with(|| {
		// We specify `Parent` because we are referencing the Relay Chain token.
		// This chain doesn't have a token of its own, so we always refer to this token,
		// and we do so by the Location of the Relay Chain.
		let fee_payment: Asset = (Location::here(), 25u128 * CENTS as u128).into();

		let xcm = Xcm(vec![
			SetAppendix(Xcm(vec![
				RefundSurplus,
				DepositAsset {
					assets: AssetFilter::Wild(WildAsset::All),
					beneficiary: Location::new(1, Parachain(SIMPLE_PARA_ID)),
				},
			])),
			WithdrawAsset(fee_payment.clone().into()),
			PayFees { asset: fee_payment },
			Transact {
				origin_kind: OriginKind::SovereignAccount,
				fallback_max_weight: None,
				call: asset_para::RuntimeCall::ForeignAssets(
					pallet_assets::Call::set_metadata {
						id: Location::new(1, Parachain(SIMPLE_PARA_ID)),
						name: "Simple Para Token".into(),
						symbol: "TOK".into(),
						decimals: 12,
					}
				).encode().into()
			}
		]);

		assert_ok!(simple_para::XcmPallet::send(
			simple_para::RuntimeOrigin::root(),
			Box::new(Location::new(1, Parachain(ASSET_PARA_ID)).into()),
			Box::new(VersionedXcm::V5(xcm)),
		));
	});

	AssetPara::execute_with(|| {
	});
}
