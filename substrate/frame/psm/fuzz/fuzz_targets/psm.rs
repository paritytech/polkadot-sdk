#![no_main]

//! Coverage-guided fuzzer for pallet-psm.
//!
//! Generates multi-block sequences of PSM dispatchables, executes them with
//! state-aware amount generation, and validates invariants via `do_try_state`
//! after each block. Uses libFuzzer's coverage feedback to explore interesting
//! call sequences.

use arbitrary::{Arbitrary, Unstructured};
use frame_support::traits::fungibles::Inspect;
use libfuzzer_sys::fuzz_target;
use pallet_psm::mock::fuzz_helpers as fh;
use pallet_psm::mock::{
	Assets, Psm, RuntimeOrigin, System, Test, ALL_EXTERNAL_ASSETS, DAI_ASSET_ID, FRAX_ASSET_ID,
	INSURANCE_FUND, PUSD_ASSET_ID, PUSD_UNIT, USDC_ASSET_ID, USDP_ASSET_ID, USDT_ASSET_ID,
};
use pallet_psm::CircuitBreakerLevel;
use sp_runtime::{BuildStorage, Permill};
use std::fs::OpenOptions;
use std::io::Write;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

// 10 accounts gives enough diversity for cross-account interactions (e.g. one account
// mints, a different one redeems) while keeping the fuzzer input compact — each account
// index fits in a u8 nibble.
const N_ACCOUNTS: u8 = 10;
const MIN_SWAP: u128 = 100 * PUSD_UNIT;
// Deliberately finite: if every account started with u128::MAX, the fuzzer could never
// discover "insufficient balance" rejection paths. 500K units is large enough to permit
// multi-transaction sequences but small enough that sustained minting can exhaust it.
const INITIAL_EXTERNAL_BALANCE: u128 = 500_000 * PUSD_UNIT;
const INITIAL_NATIVE_BALANCE: u128 = 1_000_000 * PUSD_UNIT;
// 20M units leaves headroom for ceiling exploration: with 50% MaxPsmDebtOfTotal the
// global debt cap starts at 10M, so the fuzzer can mint well past any single-asset
// ceiling without immediately hitting the issuance limit.
const MAX_PSM_ISSUANCE: u128 = 20_000_000 * PUSD_UNIT;

// ---------------------------------------------------------------------------
// Hint enums (produced by Arbitrary, no storage access)
// ---------------------------------------------------------------------------

// AmountTier is a *hint* produced during `Arbitrary` construction, where no storage is
// available. It is resolved to a concrete u128 later by `resolve_amount`, which runs inside
// externalities and can read the current ceiling/debt. Each variant targets a different
// pallet boundary condition (see `resolve_amount` for the mapping).
#[derive(Debug)]
enum AmountTier {
	MinSwap,
	NearCeiling,
	AtCeiling,
	OverCeiling,
	Random(u128),
}

// Op carries raw indices and entropy bytes rather than resolved asset IDs or amounts.
// This split is necessary because `Arbitrary` runs outside externalities — it cannot call
// into storage to enumerate approved assets or read current ceilings. The `dispatch_op`
// function resolves these opaque indices against live storage at execution time.
//
// Why full u8 for asset_idx instead of constraining to the known asset count?
// Three reasons. (1) The number of *approved* assets changes during a fuzz campaign
// (governance adds/removes them), so the count at generation time may not match the
// count at dispatch time. (2) The modulo in dispatch_op already maps every possible u8
// to a valid asset, so there is no waste — each byte always resolves correctly.
// (3) Full u8 costs the same as a constrained range (both consume one byte of fuzz
// input), but gives libFuzzer more distinct byte→coverage edges to explore.
#[derive(Debug)]
enum Op {
	Mint { account_idx: u8, asset_idx: u8, tier: AmountTier },
	Redeem { account_idx: u8, asset_idx: u8, tier: AmountTier },
	SetMaxPsmDebt { force_below_debt: bool, parts: u32 },
	SetAssetCeilingWeight { asset_idx: u8, parts: u32 },
	SetMintingFee { asset_idx: u8, parts: u32 },
	SetRedemptionFee { asset_idx: u8, parts: u32 },
	SetAssetStatus { asset_idx: u8, level: u8 },
	AddExternalAsset { asset_idx: u8 },
	RemoveExternalAsset { asset_idx: u8 },
}

// ---------------------------------------------------------------------------
// Weighted call selection (kitchensink pattern)
// ---------------------------------------------------------------------------

struct CallSpec {
	weight: u32,
	generator: fn(&mut Unstructured) -> arbitrary::Result<Op>,
}

// Mint and redeem carry the highest weight (15 each) because they are the core accounting
// paths — every other operation exists to set up interesting states for them. Governance
// ops (debt/ceiling/fee/status) get moderate weight (3-5) since they mutate state that
// subsequent mints/redeems must handle correctly. Asset management (add/remove) gets the
// lowest weight (1) because those calls are irreversible and low-yield once executed.
const CALL_SPECS: &[CallSpec] = &[
	CallSpec { weight: 15, generator: gen_mint },
	CallSpec { weight: 15, generator: gen_redeem },
	CallSpec { weight: 5, generator: gen_set_max_psm_debt },
	CallSpec { weight: 5, generator: gen_set_asset_ceiling_weight },
	CallSpec { weight: 3, generator: gen_set_minting_fee },
	CallSpec { weight: 3, generator: gen_set_redemption_fee },
	CallSpec { weight: 3, generator: gen_set_asset_status },
	CallSpec { weight: 1, generator: gen_add_external_asset },
	CallSpec { weight: 1, generator: gen_remove_external_asset },
];

fn gen_mint(u: &mut Unstructured) -> arbitrary::Result<Op> {
	Ok(Op::Mint { account_idx: u.arbitrary()?, asset_idx: u.arbitrary()?, tier: u.arbitrary()? })
}

fn gen_redeem(u: &mut Unstructured) -> arbitrary::Result<Op> {
	Ok(Op::Redeem { account_idx: u.arbitrary()?, asset_idx: u.arbitrary()?, tier: u.arbitrary()? })
}

fn gen_set_max_psm_debt(u: &mut Unstructured) -> arbitrary::Result<Op> {
	Ok(Op::SetMaxPsmDebt { force_below_debt: u.arbitrary()?, parts: u.arbitrary()? })
}

fn gen_set_asset_ceiling_weight(u: &mut Unstructured) -> arbitrary::Result<Op> {
	Ok(Op::SetAssetCeilingWeight { asset_idx: u.arbitrary()?, parts: u.arbitrary()? })
}

fn gen_set_minting_fee(u: &mut Unstructured) -> arbitrary::Result<Op> {
	Ok(Op::SetMintingFee { asset_idx: u.arbitrary()?, parts: u.arbitrary()? })
}

fn gen_set_redemption_fee(u: &mut Unstructured) -> arbitrary::Result<Op> {
	Ok(Op::SetRedemptionFee { asset_idx: u.arbitrary()?, parts: u.arbitrary()? })
}

fn gen_set_asset_status(u: &mut Unstructured) -> arbitrary::Result<Op> {
	Ok(Op::SetAssetStatus { asset_idx: u.arbitrary()?, level: u.arbitrary()? })
}

fn gen_add_external_asset(u: &mut Unstructured) -> arbitrary::Result<Op> {
	Ok(Op::AddExternalAsset { asset_idx: u.arbitrary()? })
}

fn gen_remove_external_asset(u: &mut Unstructured) -> arbitrary::Result<Op> {
	Ok(Op::RemoveExternalAsset { asset_idx: u.arbitrary()? })
}

// ---------------------------------------------------------------------------
// Arbitrary impls
// ---------------------------------------------------------------------------

// Uniform 5-way distribution ensures each tier is equally likely. Each variant targets
// a distinct pallet code path: MinSwap triggers BelowMinimumSwap, NearCeiling probes
// the boundary just below the cap, AtCeiling exercises the exact-limit path, OverCeiling
// triggers the overdraft rejection, and Random exercises the general case.
impl<'a> Arbitrary<'a> for AmountTier {
	fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
		Ok(match u.int_in_range(0..=4)? {
			0 => AmountTier::MinSwap,
			1 => AmountTier::NearCeiling,
			2 => AmountTier::AtCeiling,
			3 => AmountTier::OverCeiling,
			4 => AmountTier::Random(u.arbitrary()?),
			_ => unreachable!(),
		})
	}
}

// Weighted random selection borrowed from the substrate-runtime-fuzzer kitchensink pattern.
// A single u32 is drawn from the fuzz input and mapped to a call type proportionally,
// then the remaining bytes are consumed by that call's specific generator. This ensures
// the distribution of call types is deterministic relative to the input, which improves
// coverage guidance compared to uniform selection.
impl<'a> Arbitrary<'a> for Op {
	fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
		let total_weight: u32 = CALL_SPECS.iter().map(|s| s.weight).sum();
		let rand_value: u32 = u.int_in_range(0..=u32::MAX)?;
		let mut threshold = ((rand_value as u64 * total_weight as u64) / u32::MAX as u64) as u32;

		for spec in CALL_SPECS {
			if threshold < spec.weight {
				return (spec.generator)(u);
			}
			threshold -= spec.weight;
		}

		// Fallback; CALL_SPECS is non-empty, so this is safe.
		(CALL_SPECS[0].generator)(u)
	}
}

// ---------------------------------------------------------------------------
// Block / multi-block structure (tiered entropy from single byte)
// ---------------------------------------------------------------------------

// Tiered entropy: a single byte is split into a 5-bit tier and a 3-bit variation,
// maximizing coverage per byte of fuzz input. The tier selects a rough magnitude
// (few/medium/many calls) and the variation adds fine-grained jitter within that tier.
// Without this split, the fuzzer would waste entire bytes on a single linear range.
#[derive(Debug)]
struct Block(Vec<Op>);

impl<'a> Arbitrary<'a> for Block {
	fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
		let decision: u8 = u.arbitrary()?;
		let tier = decision & 0x1F;
		let variation = (decision >> 5) & 0x07;

		let num_calls = match tier {
			0..=15 => 1 + (variation % 3),
			16..=28 => 3 + (variation % 6),
			29..=31 => 8 + variation,
			_ => unreachable!(),
		};

		let calls = (0..num_calls).map(|_| u.arbitrary()).collect::<Result<Vec<_>, _>>()?;

		Ok(Block(calls))
	}
}

// Same tiered-entropy approach as Block, but controls the number of blocks.
// Multi-block sequences are important because governance changes in one block
// can create transient violations that subsequent blocks must resolve.
#[derive(Debug)]
struct MultiBlockOps(Vec<Block>);

impl<'a> Arbitrary<'a> for MultiBlockOps {
	fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
		let decision: u8 = u.arbitrary()?;
		let tier = decision & 0x1F;
		let variation = (decision >> 5) & 0x07;

		let num_blocks = match tier {
			0..=9 => 1 + (variation % 5) as usize,
			10..=25 => 5 + (variation % 6) as usize,
			26..=31 => 10 + ((variation as usize * 10) / 7),
			_ => unreachable!(),
		};

		let blocks = (0..num_blocks).map(|_| u.arbitrary()).collect::<Result<Vec<_>, _>>()?;

		Ok(MultiBlockOps(blocks))
	}
}

// ---------------------------------------------------------------------------
// Amount resolution (called inside externalities with storage access)
// ---------------------------------------------------------------------------

// Maps each AmountTier to a concrete amount that targets a specific pallet check:
//   MinSwap    -> exactly the pallet's minimum, may trigger BelowMinimumSwap if effective_cap < MIN_SWAP
//   NearCeiling -> one MIN_SWAP below the cap, tests the "almost full" path
//   AtCeiling  -> exact cap, tests the boundary where debt == ceiling after mint
//   OverCeiling -> cap + MIN_SWAP, triggers the OverCeiling rejection path
//   Random     -> arbitrary value in [MIN_SWAP, 2*cap], exercises the general interior
fn resolve_amount(tier: &AmountTier, effective_cap: u128) -> u128 {
	match tier {
		AmountTier::MinSwap => MIN_SWAP,
		AmountTier::NearCeiling => effective_cap.saturating_sub(MIN_SWAP).max(MIN_SWAP),
		AmountTier::AtCeiling => effective_cap,
		AmountTier::OverCeiling => effective_cap.saturating_add(MIN_SWAP),
		AmountTier::Random(raw) => {
			let upper = (2u128 * effective_cap).max(MIN_SWAP);
			let range = upper.saturating_sub(MIN_SWAP).max(1);
			MIN_SWAP.saturating_add(raw % range)
		},
	}
}

// ---------------------------------------------------------------------------
// Genesis setup (10 accounts, 5 external assets, PSM with USDC/USDT)
// ---------------------------------------------------------------------------

// Genesis creates 6 assets but only USDC and USDT are PSM-approved. The remaining
// three (DAI, USDP, FRAX) are pre-funded so the fuzzer can exercise AddExternalAsset
// without first needing to create the asset. All 5 external assets are funded across
// all 10 accounts so that newly-approved assets can be minted immediately by any account.
//
// PSM genesis: USDC gets 60% ceiling weight, USDT gets 40%, both with 1% minting and
// redemption fees. MaxPsmDebtOfTotal is 50%, meaning the total PSM debt cannot exceed
// half the pUSD issuance cap. This gives enough room for the fuzzer to explore ceiling
// violations without trivially saturating the global limit.
fn build_fuzzer_genesis() -> sp_io::TestExternalities {
	let mut storage = <frame_system::GenesisConfig<Test> as Default>::default()
		.build_storage()
		.expect("system genesis storage builds; qed");

	let accounts: Vec<u64> = (1..=N_ACCOUNTS as u64).collect();

	pallet_balances::GenesisConfig::<Test> {
		balances: accounts
			.iter()
			.map(|&a| (a, INITIAL_NATIVE_BALANCE))
			.chain(std::iter::once((INSURANCE_FUND, 1)))
			.collect(),
		..Default::default()
	}
	.assimilate_storage(&mut storage)
	.expect("balances genesis assimilates; qed");

	let asset_owner: u64 = 1;
	pallet_assets::GenesisConfig::<Test> {
		assets: vec![
			(PUSD_ASSET_ID, asset_owner, true, 1),
			(USDC_ASSET_ID, asset_owner, true, 1),
			(USDT_ASSET_ID, asset_owner, true, 1),
			(DAI_ASSET_ID, asset_owner, true, 1),
			(USDP_ASSET_ID, asset_owner, true, 1),
			(FRAX_ASSET_ID, asset_owner, true, 1),
		],
		metadata: vec![
			(PUSD_ASSET_ID, b"pUSD Stablecoin".to_vec(), b"pUSD".to_vec(), 6),
			(USDC_ASSET_ID, b"USD Coin".to_vec(), b"USDC".to_vec(), 6),
			(USDT_ASSET_ID, b"Tether USD".to_vec(), b"USDT".to_vec(), 6),
			(DAI_ASSET_ID, b"Dai Stablecoin".to_vec(), b"DAI".to_vec(), 6),
			(USDP_ASSET_ID, b"Pax Dollar".to_vec(), b"USDP".to_vec(), 6),
			(FRAX_ASSET_ID, b"Frax".to_vec(), b"FRAX".to_vec(), 6),
		],
		accounts: accounts
			.iter()
			.flat_map(|&a| {
				ALL_EXTERNAL_ASSETS.iter().map(move |&id| (id, a, INITIAL_EXTERNAL_BALANCE))
			})
			.collect(),
		..Default::default()
	}
	.assimilate_storage(&mut storage)
	.expect("assets genesis assimilates; qed");

	pallet_psm::GenesisConfig::<Test> {
		max_psm_debt_of_total: Permill::from_percent(50),
		asset_configs: [
			(
				USDC_ASSET_ID,
				(Permill::from_percent(1), Permill::from_percent(1), Permill::from_percent(60)),
			),
			(
				USDT_ASSET_ID,
				(Permill::from_percent(1), Permill::from_percent(1), Permill::from_percent(40)),
			),
		]
		.into_iter()
		.collect(),
		_marker: Default::default(),
	}
	.assimilate_storage(&mut storage)
	.expect("PSM genesis assimilates; qed");

	let mut ext: sp_io::TestExternalities = storage.into();
	ext.execute_with(|| {
		System::set_block_number(1);
		pallet_psm::mock::set_mock_maximum_issuance(MAX_PSM_ISSUANCE);
	});
	ext
}

// ---------------------------------------------------------------------------
// Dispatch (runs inside externalities, reads storage via fh::*)
// ---------------------------------------------------------------------------

fn dispatch_op(op: &Op) {
	match op {
		Op::Mint { account_idx, asset_idx, tier } => {
			// The effective cap is the minimum of four independent limits that must
			// all be satisfied for a mint to succeed: per-asset ceiling remaining,
			// global PSM debt remaining, pUSD issuance remaining, and the caller's
			// external-asset balance. Taking the min ensures the fuzzer generates
			// amounts that are feasible under at least one limit, letting coverage
			// guidance discover which limit is the binding constraint.
			let asset_id =
				ALL_EXTERNAL_ASSETS[(*asset_idx % ALL_EXTERNAL_ASSETS.len() as u8) as usize];
			let account = (*account_idx % N_ACCOUNTS + 1) as u64;
			if !fh::is_approved_asset(asset_id) {
				return;
			}
			let debt = fh::psm_debt(asset_id);
			let ceiling = fh::max_asset_debt(asset_id);
			let remaining = ceiling.saturating_sub(debt);
			let global_remaining = fh::max_psm_debt().saturating_sub(fh::total_psm_debt());
			let balance = Assets::balance(asset_id, account);
			let issuance_remaining = pallet_psm::mock::MockMaximumIssuance::get()
				.saturating_sub(Assets::total_issuance(PUSD_ASSET_ID));
			let effective_cap =
				remaining.min(global_remaining).min(issuance_remaining).min(balance);
			let amount = resolve_amount(tier, effective_cap);
			if amount >= MIN_SWAP {
				let _ = Psm::mint(RuntimeOrigin::signed(account), asset_id, amount);
			}
		},
		Op::Redeem { account_idx, asset_idx, tier } => {
			let asset_id =
				ALL_EXTERNAL_ASSETS[(*asset_idx % ALL_EXTERNAL_ASSETS.len() as u8) as usize];
			let account = (*account_idx % N_ACCOUNTS + 1) as u64;
			if !fh::is_approved_asset(asset_id) {
				return;
			}
			let debt = fh::psm_debt(asset_id);
			let user_pusd = Assets::balance(PUSD_ASSET_ID, account);
			let effective_cap = debt.min(user_pusd);
			let amount = resolve_amount(tier, effective_cap);
			if amount >= MIN_SWAP {
				let _ = Psm::redeem(RuntimeOrigin::signed(account), asset_id, amount);
			}
		},
		// When force_below_debt is true, the ratio is computed so that max_psm_debt ends up
		// below the current total_psm_debt, creating a transient invariant violation. This
		// stresses the pallet's handling of the state where governance has overshot —
		// subsequent mints must be rejected, and the violation must persist until either
		// debt is redeemed or the ceiling is raised.
		Op::SetMaxPsmDebt { force_below_debt, parts } => {
			let ratio = if *force_below_debt {
				let debt = fh::total_psm_debt();
				let max_issuance = pallet_psm::mock::MockMaximumIssuance::get();
				if debt == 0 || max_issuance == 0 {
					Permill::from_parts(parts % 1_000_001)
				} else {
					let below_debt = Permill::from_rational(debt / 2, max_issuance);
					below_debt.min(Permill::from_parts(parts % below_debt.deconstruct() as u32))
				}
			} else {
				Permill::from_parts(parts % 1_000_001)
			};
			let _ = Psm::set_max_psm_debt(RuntimeOrigin::root(), ratio);
		},
		Op::SetAssetCeilingWeight { asset_idx, parts } => {
			let asset_id =
				ALL_EXTERNAL_ASSETS[(*asset_idx % ALL_EXTERNAL_ASSETS.len() as u8) as usize];
			let weight = Permill::from_parts(parts % 1_000_001);
			let _ = Psm::set_asset_ceiling_weight(RuntimeOrigin::root(), asset_id, weight);
		},
		Op::SetMintingFee { asset_idx, parts } => {
			let asset_id =
				ALL_EXTERNAL_ASSETS[(*asset_idx % ALL_EXTERNAL_ASSETS.len() as u8) as usize];
			let fee = Permill::from_parts(parts % 1_000_001);
			let _ = Psm::set_minting_fee(RuntimeOrigin::root(), asset_id, fee);
		},
		Op::SetRedemptionFee { asset_idx, parts } => {
			let asset_id =
				ALL_EXTERNAL_ASSETS[(*asset_idx % ALL_EXTERNAL_ASSETS.len() as u8) as usize];
			let fee = Permill::from_parts(parts % 1_000_001);
			let _ = Psm::set_redemption_fee(RuntimeOrigin::root(), asset_id, fee);
		},
		Op::SetAssetStatus { asset_idx, level } => {
			let asset_id =
				ALL_EXTERNAL_ASSETS[(*asset_idx % ALL_EXTERNAL_ASSETS.len() as u8) as usize];
			if !fh::is_approved_asset(asset_id) {
				return;
			}
			let status = match level % 3 {
				0 => CircuitBreakerLevel::AllEnabled,
				1 => CircuitBreakerLevel::MintingDisabled,
				_ => CircuitBreakerLevel::AllDisabled,
			};
			let _ = Psm::set_asset_status(RuntimeOrigin::root(), asset_id, status);
		},
		// Newly-added assets receive a non-zero ceiling weight immediately via the paired
		// set_asset_ceiling_weight call. Without this, a freshly-added asset has zero ceiling
		// and every subsequent mint would fail trivially with OverCeiling — producing no
		// useful coverage signal.
		Op::AddExternalAsset { asset_idx } => {
			let candidates: Vec<u32> = ALL_EXTERNAL_ASSETS
				.iter()
				.filter(|&&id| !fh::is_approved_asset(id))
				.copied()
				.collect();
			if candidates.is_empty() {
				return;
			}
			let asset_id = candidates[(*asset_idx as usize) % candidates.len()];
			if Psm::add_external_asset(RuntimeOrigin::root(), asset_id).is_ok() {
				let weight = Permill::from_parts(((*asset_idx as u32) % 1_000_001).max(1));
				let _ = Psm::set_asset_ceiling_weight(RuntimeOrigin::root(), asset_id, weight);
			}
		},
		Op::RemoveExternalAsset { asset_idx } => {
			let candidates: Vec<u32> =
				fh::approved_assets().into_iter().filter(|&id| fh::psm_debt(id) == 0).collect();
			if candidates.is_empty() {
				return;
			}
			let asset_id = candidates[(*asset_idx as usize) % candidates.len()];
			let _ = Psm::remove_external_asset(RuntimeOrigin::root(), asset_id);
		},
	}
}

// ---------------------------------------------------------------------------
// Fuzzer entry point
// ---------------------------------------------------------------------------

// Each fuzz input produces a multi-block sequence. Within each block, all ops execute
// against the same block number, then do_try_state validates pallet invariants. The
// check runs per-block rather than per-call because PSM invariants are defined at block
// boundaries — governance can create transient violations mid-block (e.g. lowering the
// ceiling below current debt) that are resolved by subsequent calls within the same block.
fuzz_target!(|input: MultiBlockOps| {
	let mut ext = build_fuzzer_genesis();
	let mut block_number: u32 = 1;

	// Log generated ops for post-mortem debugging.
	if let Ok(mut log_file) = OpenOptions::new().create(true).append(true).open("psm-fuzz.log") {
		let _ = writeln!(log_file, "\n{}", "=".repeat(80));
		let _ = writeln!(log_file, "NEW FUZZER INPUT - {} blocks", input.0.len());
		let _ = writeln!(log_file, "{}", "=".repeat(80));
		for (bi, block) in input.0.iter().enumerate() {
			let _ = writeln!(log_file, "Block {} ({} ops):", bi + 1, block.0.len());
			for (oi, op) in block.0.iter().enumerate() {
				let _ = writeln!(log_file, "  Op {}: {:?}", oi + 1, op);
			}
		}
	}

	for block in input.0.iter() {
		ext.execute_with(|| {
			System::set_block_number(block_number.into());
			for op in &block.0 {
				dispatch_op(op);
			}
			fh::do_try_state().expect("PSM invariant violated");
		});
		block_number += 1;
	}
});
