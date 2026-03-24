#![no_main]

use arbitrary::Arbitrary;
use frame_support::traits::fungibles::Mutate;
use libfuzzer_sys::fuzz_target;
use pallet_psm::mock::{
	new_test_ext, Assets, Psm, RuntimeOrigin, Test, ALICE, PUSD_UNIT, USDC_ASSET_ID, USDT_ASSET_ID,
};
use pallet_psm::CircuitBreakerLevel;
use sp_runtime::Permill;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

const MIN_SWAP: u128 = PUSD_UNIT * 100;
const N_ASSETS: u8 = 2;
const ASSET_IDS: [u32; 2] = [USDC_ASSET_ID, USDT_ASSET_ID];

#[derive(Debug, Arbitrary)]
enum Op {
	Mint { asset_idx: u8, multiplier: u16 },
	Redeem { asset_idx: u8, multiplier: u16 },
	SetMintingFee { asset_idx: u8, fee_parts: u32 },
	SetRedemptionFee { asset_idx: u8, fee_parts: u32 },
	SetMaxPsmDebt { parts: u32 },
	SetAssetStatus { asset_idx: u8, level: u8 },
	SetAssetCeilingWeight { asset_idx: u8, parts: u32 },
}

static ITER_COUNT: AtomicU64 = AtomicU64::new(0);

thread_local! {
	static STATS: RefCell<HashMap<&'static str, (u64, u64)>> = RefCell::new(HashMap::new());
	static ERRORS: RefCell<HashMap<(&'static str, String), u64>> = RefCell::new(HashMap::new());
	static PRINT_ON_DROP: PrintOnDrop = const { PrintOnDrop };
}

struct PrintOnDrop;

impl Drop for PrintOnDrop {
	fn drop(&mut self) {
		print_stats();
	}
}

fn record_ok(op: &'static str) {
	STATS.with(|s| {
		s.borrow_mut().entry(op).or_insert((0, 0)).0 += 1;
	});
}

fn record_err(op: &'static str, err: impl core::fmt::Debug) {
	STATS.with(|s| {
		s.borrow_mut().entry(op).or_insert((0, 0)).1 += 1;
	});
	let key = format!("{err:?}");
	ERRORS.with(|e| {
		*e.borrow_mut().entry((op, key)).or_insert(0) += 1;
	});
}

fn maybe_print_stats() {
	let n = ITER_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
	if n % 10_000 == 0 {
		print_stats();
	}
}

fn print_stats() {
	STATS.with(|s| {
		let s = s.borrow();
		let mut ops: Vec<_> = s.iter().collect();
		ops.sort_by_key(|(k, _)| *k);
		eprintln!("\n=== PSM Fuzz Stats ===");
		for (op, (ok, err)) in &ops {
			eprintln!("  {}: {} ok / {} err", op, ok, err);
		}
	});
	ERRORS.with(|e| {
		let e = e.borrow();
		let mut sorted: Vec<_> = e.iter().collect();
		sorted.sort_by(|a, b| b.1.cmp(a.1));
		eprintln!("  Top errors:");
		for ((op, err), count) in sorted.iter().take(10) {
			eprintln!("    {}: {} ({})", op, err, count);
		}
	});
}

fuzz_target!(|ops: Vec<Op>| {
	new_test_ext().execute_with(|| {
		PRINT_ON_DROP.with(|_| {});

		let _ = Assets::mint_into(USDC_ASSET_ID, &ALICE, u128::MAX / 2);
		let _ = Assets::mint_into(USDT_ASSET_ID, &ALICE, u128::MAX / 2);

		for op in ops {
			match op {
				Op::Mint { asset_idx, multiplier } => {
					let asset_id = ASSET_IDS[(asset_idx % N_ASSETS) as usize];
					let amount = (multiplier as u128 + 1).saturating_mul(MIN_SWAP);
					match Psm::mint(RuntimeOrigin::signed(ALICE), asset_id, amount) {
						Ok(_) => record_ok("Mint"),
						Err(e) => record_err("Mint", e),
					}
				},
				Op::Redeem { asset_idx, multiplier } => {
					let asset_id = ASSET_IDS[(asset_idx % N_ASSETS) as usize];
					let amount = (multiplier as u128 + 1).saturating_mul(MIN_SWAP);
					match Psm::redeem(RuntimeOrigin::signed(ALICE), asset_id, amount) {
						Ok(_) => record_ok("Redeem"),
						Err(e) => record_err("Redeem", e),
					}
				},
				Op::SetMintingFee { asset_idx, fee_parts } => {
					let asset_id = ASSET_IDS[(asset_idx % N_ASSETS) as usize];
					let fee = Permill::from_parts(fee_parts % 1_000_001);
					match Psm::set_minting_fee(RuntimeOrigin::root(), asset_id, fee) {
						Ok(_) => record_ok("SetMintingFee"),
						Err(e) => record_err("SetMintingFee", e),
					}
				},
				Op::SetRedemptionFee { asset_idx, fee_parts } => {
					let asset_id = ASSET_IDS[(asset_idx % N_ASSETS) as usize];
					let fee = Permill::from_parts(fee_parts % 1_000_001);
					match Psm::set_redemption_fee(RuntimeOrigin::root(), asset_id, fee) {
						Ok(_) => record_ok("SetRedemptionFee"),
						Err(e) => record_err("SetRedemptionFee", e),
					}
				},
				Op::SetMaxPsmDebt { parts } => {
					let ratio = Permill::from_parts(parts % 1_000_001);
					match Psm::set_max_psm_debt(RuntimeOrigin::root(), ratio) {
						Ok(_) => record_ok("SetMaxPsmDebt"),
						Err(e) => record_err("SetMaxPsmDebt", e),
					}
				},
				Op::SetAssetStatus { asset_idx, level } => {
					let asset_id = ASSET_IDS[(asset_idx % N_ASSETS) as usize];
					let status = match level % 3 {
						0 => CircuitBreakerLevel::AllEnabled,
						1 => CircuitBreakerLevel::MintingDisabled,
						_ => CircuitBreakerLevel::AllDisabled,
					};
					match Psm::set_asset_status(RuntimeOrigin::root(), asset_id, status) {
						Ok(_) => record_ok("SetAssetStatus"),
						Err(e) => record_err("SetAssetStatus", e),
					}
				},
				Op::SetAssetCeilingWeight { asset_idx, parts } => {
					let asset_id = ASSET_IDS[(asset_idx % N_ASSETS) as usize];
					let weight = Permill::from_parts(parts % 1_000_001);
					match Psm::set_asset_ceiling_weight(RuntimeOrigin::root(), asset_id, weight) {
						Ok(_) => record_ok("SetAssetCeilingWeight"),
						Err(e) => record_err("SetAssetCeilingWeight", e),
					}
				},
			}

			pallet_psm::Pallet::<Test>::do_try_state()
				.expect("PSM invariant violated after fuzz op");
		}

		maybe_print_stats();
	});
});
