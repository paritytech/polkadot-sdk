//! Quint model-based testing driver for pallet-psm.
//!
//! Generates traces from `model/psm.qnt` and replays them against the real
//! pallet-psm via `TestExternalities`. After each step the framework compares
//! the model's `(debt, reserve)` against the pallet's `PsmDebt::<T>::get(USDT)`
//! and `Assets::balance(USDT, psm_account)` and fails on divergence.

use std::cell::RefCell;

use pallet_psm::mock::{
	psm_account, Assets, Psm, RuntimeOrigin, Test, ALICE, INTERNAL_ASSET_ID, INTERNAL_UNIT,
	USDT_ASSET_ID,
};
use pallet_psm::PsmDebt;
use pallet_psm_fuzz::build_fuzzer_genesis;
use quint_connect::*;
use serde::Deserialize;
use sp_runtime::Permill;

// ---------------------------------------------------------------------------
// State — mirrors the variables in psm.qnt
// ---------------------------------------------------------------------------
//
// The Quint spec declares two `int` variables: `debt` and `reserve`.
// quint-connect deserializes them from the ITF trace into this struct, then
// asks the Driver to produce the same struct from its own state for comparison.

#[derive(Eq, PartialEq, Deserialize, Debug)]
struct PsmState {
	debt: i128,
	reserve: i128,
}

impl State<PsmDriver> for PsmState {
	fn from_driver(driver: &PsmDriver) -> Result<Self> {
		// Pallet reads need `execute_with`, which takes `&mut TestExternalities`.
		// The Driver method here gets `&self`, so we go through RefCell.
		let mut ext = driver.ext.borrow_mut();
		let (debt, reserve) = ext.execute_with(|| {
			let debt = PsmDebt::<Test>::get(INTERNAL_ASSET_ID, USDT_ASSET_ID);
			let reserve = Assets::balance(USDT_ASSET_ID, &psm_account());
			(debt, reserve)
		});
		// Translate pallet units back to abstract Quint units for comparison.
		Ok(PsmState { debt: (debt / SCALE) as i128, reserve: (reserve / SCALE) as i128 })
	}
}

// ---------------------------------------------------------------------------
// Driver — owns the pallet's TestExternalities, runs Quint actions against it
// ---------------------------------------------------------------------------

// Scale factor between abstract Quint amounts (1, 2, 3, ...) and pallet units.
// One abstract unit equals MinSwapAmount, so amount=1 is the smallest legal mint.
const SCALE: u128 = 100 * INTERNAL_UNIT;

struct PsmDriver {
	ext: RefCell<sp_io::TestExternalities>,
}

fn fresh_ext() -> sp_io::TestExternalities {
	let mut ext = build_fuzzer_genesis();
	// The PoC model assumes zero fees; the fuzz crate's genesis sets 1%.
	// Override here so model state matches pallet state exactly.
	ext.execute_with(|| {
		Psm::set_minting_fee(RuntimeOrigin::root(), INTERNAL_ASSET_ID, USDT_ASSET_ID, Permill::zero())
			.expect("root can zero the minting fee");
		Psm::set_redemption_fee(RuntimeOrigin::root(), INTERNAL_ASSET_ID, USDT_ASSET_ID, Permill::zero())
			.expect("root can zero the redemption fee");
	});
	ext
}

impl Default for PsmDriver {
	fn default() -> Self {
		PsmDriver { ext: RefCell::new(fresh_ext()) }
	}
}

impl Driver for PsmDriver {
	type State = PsmState;

	fn step(&mut self, step: &Step) -> Result {
		switch!(step {
			init => {
				*self.ext.borrow_mut() = fresh_ext();
			},
			mint(amount: i128) => {
				let mut ext = self.ext.borrow_mut();
				ext.execute_with(|| {
					Psm::mint(
						RuntimeOrigin::signed(ALICE),
						INTERNAL_ASSET_ID,
						USDT_ASSET_ID,
						(amount as u128) * SCALE,
						Permill::zero(),
					)
					.expect("mint should succeed; the Quint guard ensures preconditions");
				});
			},
			redeem(amount: i128) => {
				let mut ext = self.ext.borrow_mut();
				ext.execute_with(|| {
					Psm::redeem(
						RuntimeOrigin::signed(ALICE),
						INTERNAL_ASSET_ID,
						USDT_ASSET_ID,
						(amount as u128) * SCALE,
						Permill::zero(),
					)
					.expect("redeem should succeed; the Quint guard ensures preconditions");
				});
			},
		})
	}
}

// ---------------------------------------------------------------------------
// Bidirectional counterfactual (see docs/case-bidirectional-debt.md)
// ---------------------------------------------------------------------------
//
// The bidirectional spec tracks cumulative inflow and outflow alongside debt
// and reserve. Run against the unmodified pallet, all four match.
//
// When run with `--features inject-debt-understatement-bug`, the harness
// switches on the pallet's runtime bug toggle, and the pallet
// subtracts one less than `effective_internal_net` from PsmDebt on every
// redeem. The spec's debt then diverges from the pallet's PsmDebt on the
// first redeem of size >= 2 — caught by the state comparison — while the
// upper-bound invariants (reserve >= debt, issuance >= debt) still hold on
// both sides. This is the case made by docs/case-bidirectional-debt.md.

#[derive(Eq, PartialEq, Deserialize, Debug)]
struct BidirectionalState {
	debt: i128,
	reserve: i128,
	inflow: i128,
	outflow: i128,
}

struct BidirectionalDriver {
	ext: RefCell<sp_io::TestExternalities>,
	inflow: u128,
	outflow: u128,
}

impl Default for BidirectionalDriver {
	fn default() -> Self {
		#[cfg(feature = "inject-debt-understatement-bug")]
		pallet_psm::bug_injection::set_understate_debt_on_redeem(true);
		BidirectionalDriver { ext: RefCell::new(fresh_ext()), inflow: 0, outflow: 0 }
	}
}

impl State<BidirectionalDriver> for BidirectionalState {
	fn from_driver(driver: &BidirectionalDriver) -> Result<Self> {
		let mut ext = driver.ext.borrow_mut();
		let (debt, reserve) = ext.execute_with(|| {
			let debt = PsmDebt::<Test>::get(INTERNAL_ASSET_ID, USDT_ASSET_ID);
			let reserve = Assets::balance(USDT_ASSET_ID, &psm_account());
			(debt, reserve)
		});
		// Compare in pallet units. The bug-injection feature understates
		// PsmDebt by 1 pallet unit per redeem; if we divided by SCALE the
		// difference would round to zero and the test would silently pass.
		Ok(BidirectionalState {
			debt: debt as i128,
			reserve: reserve as i128,
			inflow: driver.inflow as i128,
			outflow: driver.outflow as i128,
		})
	}
}

impl Driver for BidirectionalDriver {
	type State = BidirectionalState;

	fn step(&mut self, step: &Step) -> Result {
		switch!(step {
			init => {
				*self.ext.borrow_mut() = fresh_ext();
				self.inflow = 0;
				self.outflow = 0;
			},
			mint(amount: i128) => {
				let raw = amount as u128;
				let mut ext = self.ext.borrow_mut();
				ext.execute_with(|| {
					Psm::mint(
						RuntimeOrigin::signed(ALICE),
						INTERNAL_ASSET_ID,
						USDT_ASSET_ID,
						raw,
						Permill::zero(),
					)
					.expect("mint should succeed; Quint guard ensures preconditions");
				});
				self.inflow = self.inflow.saturating_add(raw);
			},
			redeem(amount: i128) => {
				let raw = amount as u128;
				let mut ext = self.ext.borrow_mut();
				ext.execute_with(|| {
					Psm::redeem(
						RuntimeOrigin::signed(ALICE),
						INTERNAL_ASSET_ID,
						USDT_ASSET_ID,
						raw,
						Permill::zero(),
					)
					.expect("redeem should succeed; Quint guard ensures preconditions");
				});
				self.outflow = self.outflow.saturating_add(raw);
			},
		})
	}
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[quint_run(spec = "model/psm.qnt", main = "psm", max_samples = 50)]
fn simulation() -> impl Driver {
	PsmDriver::default()
}

#[quint_run(spec = "model/psm_bidirectional.qnt", main = "psm_bidirectional", max_samples = 50)]
fn bidirectional() -> impl Driver {
	BidirectionalDriver::default()
}
