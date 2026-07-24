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
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use super::*;

use crate::{
	mock::{new_test_ext, Coretime, OnDemand, RuntimeOrigin, System, Test},
	on_demand::mock_helpers::GenesisConfigBuilder,
};
use frame_support::{assert_noop, assert_ok};
use sp_runtime::traits::BadOrigin;

#[test]
fn place_order_origin_gating() {
	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		System::set_block_number(1);
		let para_id = ParaId::from(111);
		let ordered_by = 1u64;
		let spot_price: BalanceOf<Test> = 10_000;

		// A signed origin is not enough.
		assert_noop!(
			Coretime::place_order(RuntimeOrigin::signed(1), para_id, ordered_by, spot_price),
			BadOrigin
		);

		// Neither is a parachain origin that is not the broker chain.
		assert_noop!(
			Coretime::place_order(
				Origin::Parachain(9.into()).into(),
				para_id,
				ordered_by,
				spot_price
			),
			Error::<Test>::NotBroker
		);

		// The broker chain (BrokerId = 10 in the mock) and root are accepted.
		assert_ok!(Coretime::place_order(
			Origin::Parachain(10.into()).into(),
			para_id,
			ordered_by,
			spot_price
		));
		assert_ok!(Coretime::place_order(RuntimeOrigin::root(), para_id, ordered_by, spot_price));
	});
}

#[test]
fn place_order_enqueues_on_demand_order() {
	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		System::set_block_number(1);
		let para_id = ParaId::from(111);
		let ordered_by = 1u64;
		let spot_price: BalanceOf<Test> = 10_000;

		assert_ok!(Coretime::place_order(
			Origin::Parachain(10.into()).into(),
			para_id,
			ordered_by,
			spot_price
		));

		// The order was enqueued (and the spot price passed through verbatim)...
		System::assert_has_event(
			on_demand::Event::<Test>::OnDemandOrderPlaced { para_id, spot_price, ordered_by }
				.into(),
		);
		let mut queue = OnDemand::peek_order_queue();
		let popped: Vec<ParaId> = queue.pop_assignment_for_cores::<Test>(3, 1).collect();
		assert_eq!(popped, vec![para_id]);

		// ...but no revenue was recorded on the relay chain: the payment stayed on the
		// coretime chain and never reached the on-demand pot.
		assert_eq!(OnDemand::claim_revenue_until(2), 0);
	});
}
