//! Stateful property-based tester for pallet-psm.
//!
//! Generates semantically rich, state-aware command sequences that stress PSM
//! invariants. Reads pallet storage between each generated command to produce
//! domain-informed inputs. Runs `do_try_state()` after every command and
//! panics on violation. Uses deterministic seeding for reproducibility.
//!
//! Usage: `psm_stateful [seed] [max_commands]`

use frame_support::traits::fungibles::Inspect;
use pallet_psm::mock::fuzz_helpers as fh;
use pallet_psm::mock::{
	set_mock_maximum_issuance, Assets, MockMaximumIssuance, Psm, RuntimeOrigin, System, Test,
	ALL_EXTERNAL_ASSETS, DAI_ASSET_ID, FRAX_ASSET_ID, INSURANCE_FUND, PUSD_ASSET_ID, PUSD_UNIT,
	USDC_ASSET_ID, USDP_ASSET_ID, USDT_ASSET_ID,
};
use pallet_psm::CircuitBreakerLevel;
use rand::seq::SliceRandom;
use rand::{rngs::StdRng, Rng, SeedableRng};
use sp_io::TestExternalities;
use sp_runtime::{BuildStorage, Permill};
use std::env;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

// Constants match psm.rs but are duplicated here because this is a separate binary
// with no shared state. The values have the same rationale: finite balances to allow
// the fuzzer to discover rejection paths, and a 20M issuance cap for ceiling headroom.
const N_ACCOUNTS: u8 = 10;
const MIN_SWAP: u128 = 100 * PUSD_UNIT;
const INITIAL_EXTERNAL_BALANCE: u128 = 500_000 * PUSD_UNIT;
const INITIAL_NATIVE_BALANCE: u128 = 1_000_000 * PUSD_UNIT;
const MAX_PSM_ISSUANCE: u128 = 20_000_000 * PUSD_UNIT;

// ---------------------------------------------------------------------------
// Command enum
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum Command {
	Mint { account: u64, asset_id: u32, amount: u128 },
	Redeem { account: u64, asset_id: u32, amount: u128 },
	SetMaxPsmDebt { ratio: Permill },
	SetAssetCeilingWeight { asset_id: u32, weight: Permill },
	SetMintingFee { asset_id: u32, fee: Permill },
	SetRedemptionFee { asset_id: u32, fee: Permill },
	SetAssetStatus { asset_id: u32, status: CircuitBreakerLevel },
	AddExternalAsset { asset_id: u32, weight: Permill },
	RemoveExternalAsset { asset_id: u32 },
}

// ---------------------------------------------------------------------------
// State snapshot structs
// ---------------------------------------------------------------------------

// FuzzState captures ALL relevant pallet state in a single snapshot so that generators
// can make informed decisions. Per-account balances are included so generators can pick
// accounts that actually have sufficient funds to complete an operation, rather than
// wasting commands on trivially-failing mints from empty accounts.
#[derive(Debug)]
#[allow(dead_code)]
struct AssetState {
	asset_id: u32,
	debt: u128,
	ceiling: u128,
	remaining_ceiling: u128,
	reserve: u128,
	minting_fee: Permill,
	redemption_fee: Permill,
	weight: Permill,
	account_external: Vec<u128>,
	account_pusd: Vec<u128>,
}

#[derive(Debug)]
#[allow(dead_code)]
struct FuzzState {
	assets: Vec<AssetState>,
	unapproved: Vec<u32>,
	max_psm_debt: u128,
	total_psm_debt: u128,
	max_psm_debt_ratio: Permill,
	max_issuance: u128,
	total_pusd_issuance: u128,
	block_number: u32,
}

// ---------------------------------------------------------------------------
// FuzzState helper predicates
// ---------------------------------------------------------------------------

impl FuzzState {
	fn any_asset_can_mint(&self) -> bool {
		self.assets.iter().any(|a| {
			a.remaining_ceiling >= MIN_SWAP && a.account_external.iter().any(|&b| b >= MIN_SWAP)
		})
	}

	fn any_asset_can_redeem(&self) -> bool {
		self.assets
			.iter()
			.any(|a| a.debt >= MIN_SWAP && a.account_pusd.iter().any(|&b| b >= MIN_SWAP))
	}

	fn any_asset_near_ceiling(&self) -> bool {
		self.assets
			.iter()
			.any(|a| a.ceiling > 0 && a.remaining_ceiling <= a.ceiling / 10)
	}
}

// ---------------------------------------------------------------------------
// Genesis setup (copied from psm.rs — separate binary, no shared state)
// ---------------------------------------------------------------------------

// Genesis mirrors psm.rs exactly: 6 assets created, 2 PSM-approved (USDC 60%, USDT 40%),
// 3 unapproved for AddExternalAsset testing, all funded across all accounts.
// See psm.rs build_fuzzer_genesis comments for the full rationale.
fn build_fuzzer_genesis() -> TestExternalities {
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

	let mut ext: TestExternalities = storage.into();
	ext.execute_with(|| {
		System::set_block_number(1);
		set_mock_maximum_issuance(MAX_PSM_ISSUANCE);
	});
	ext
}

// ---------------------------------------------------------------------------
// State snapshot — reads ALL relevant storage before each command
// ---------------------------------------------------------------------------

fn snapshot_state() -> FuzzState {
	let approved = fh::approved_assets();
	let max_issuance = MockMaximumIssuance::get();
	let total_pusd_issuance = Assets::total_issuance(PUSD_ASSET_ID);

	let mut assets = Vec::with_capacity(approved.len());
	for &asset_id in &approved {
		let debt = fh::psm_debt(asset_id);
		let ceiling = fh::max_asset_debt(asset_id);
		let remaining_ceiling = ceiling.saturating_sub(debt);
		let reserve = fh::get_reserve(asset_id);
		let minting_fee = fh::minting_fee(asset_id);
		let redemption_fee = fh::redemption_fee(asset_id);
		let weight = fh::asset_ceiling_weight(asset_id);

		let mut account_external = Vec::with_capacity(N_ACCOUNTS as usize);
		let mut account_pusd = Vec::with_capacity(N_ACCOUNTS as usize);
		for account in 1..=N_ACCOUNTS as u64 {
			account_external.push(Assets::balance(asset_id, account));
			account_pusd.push(Assets::balance(PUSD_ASSET_ID, account));
		}

		assets.push(AssetState {
			asset_id,
			debt,
			ceiling,
			remaining_ceiling,
			reserve,
			minting_fee,
			redemption_fee,
			weight,
			account_external,
			account_pusd,
		});
	}

	let unapproved: Vec<u32> = ALL_EXTERNAL_ASSETS
		.iter()
		.filter(|&&id| !fh::is_approved_asset(id))
		.copied()
		.collect();

	// Test block numbers always fit in u32; qed
	let block_number: u32 = TryInto::<u32>::try_into(System::block_number()).unwrap_or(0);

	FuzzState {
		assets,
		unapproved,
		max_psm_debt: fh::max_psm_debt(),
		total_psm_debt: fh::total_psm_debt(),
		max_psm_debt_ratio: fh::max_psm_debt_ratio(),
		max_issuance,
		total_pusd_issuance,
		block_number,
	}
}

// ---------------------------------------------------------------------------
// Weighted selection helpers
// ---------------------------------------------------------------------------

// Weighted selection: assets with more remaining capacity (or debt, depending on weight_fn)
// are picked more often. This biases toward operations that will actually change state
// rather than trivially fail, making each fuzz command more likely to advance coverage.
fn pick_asset_weighted<F>(rng: &mut StdRng, state: &FuzzState, weight_fn: F) -> usize
where
	F: Fn(&AssetState) -> u128,
{
	// Caller guarantees assets non-empty; qed
	if state.assets.is_empty() {
		return 0;
	}
	let weights: Vec<u128> = state.assets.iter().map(|a| weight_fn(a).max(1)).collect();
	let total: u128 = weights.iter().sum();
	if total == 0 {
		return 0;
	}
	let mut pick = rng.gen_range(0..total);
	for (i, w) in weights.iter().enumerate() {
		if pick < *w {
			return i;
		}
		pick -= w;
	}
	0
}

// Same weighted selection for accounts: accounts with higher balances are more likely
// to produce interesting mints (they can actually complete the operation). Accounts with
// zero balance still have weight 1 (via .max(1)) so they are occasionally selected,
// exercising the insufficient-balance rejection path.
fn pick_richest_account(rng: &mut StdRng, balances: &[u128]) -> (u64, u128) {
	// balances always has N_ACCOUNTS entries; qed
	if balances.is_empty() {
		return (1, 0);
	}
	let weights: Vec<u128> = balances.iter().map(|&b| b.max(1)).collect();
	let total: u128 = weights.iter().sum();
	if total == 0 {
		return (1, 0);
	}
	let mut pick = rng.gen_range(0..total);
	for (i, w) in weights.iter().enumerate() {
		if pick < *w {
			return ((i + 1) as u64, balances[i]);
		}
		pick -= w;
	}
	(1, balances[0])
}

// ---------------------------------------------------------------------------
// Boundary-aware amount pickers
// ---------------------------------------------------------------------------

// 10-way boundary distribution: each variant targets a specific pallet check path.
// Variant 0: BelowMinimumSwap minimum. 1: exact ceiling hit. 2: one step below ceiling.
// 3: overdraft past ceiling. 4-5: mid-range (half/third). 6-7: off-by-one boundaries.
// 8: uniform random within valid range. 9: near-boundary with random offset.
fn pick_mint_amount(
	rng: &mut StdRng,
	asset: &AssetState,
	account_balance: u128,
	state: &FuzzState,
) -> u128 {
	let remaining = asset.remaining_ceiling;
	let global_remaining = state.max_psm_debt.saturating_sub(state.total_psm_debt);
	let issuance_remaining = state.max_issuance.saturating_sub(state.total_pusd_issuance);
	let effective_cap =
		remaining.min(global_remaining).min(issuance_remaining).min(account_balance);

	if effective_cap < MIN_SWAP {
		return MIN_SWAP; // BelowMinimumSwap path
	}

	match rng.gen_range(0..=9) {
		0 => MIN_SWAP,                                             // smallest valid
		1 => effective_cap,                                        // exact ceiling hit
		2 => effective_cap.saturating_sub(MIN_SWAP).max(MIN_SWAP), // one step below
		3 => effective_cap.saturating_add(MIN_SWAP),               // overdraft
		4 => effective_cap / 2,                                    // half
		5 => effective_cap / 3,                                    // third
		6 => effective_cap.saturating_sub(1).max(MIN_SWAP),        // off-by-one below
		7 => effective_cap.saturating_add(1),                      // off-by-one over
		8 => rng.gen_range(MIN_SWAP..=effective_cap),              // uniform in range
		_ => effective_cap.saturating_sub(rng.gen_range(0..MIN_SWAP)).max(MIN_SWAP), // near boundary
	}
}

fn pick_redeem_amount(rng: &mut StdRng, asset: &AssetState, pusd_balance: u128) -> u128 {
	let effective_cap = asset.debt.min(pusd_balance);

	if effective_cap < MIN_SWAP {
		return MIN_SWAP; // BelowMinimumSwap path
	}

	match rng.gen_range(0..=7) {
		0 => MIN_SWAP,
		1 => effective_cap,
		2 => effective_cap.saturating_sub(MIN_SWAP).max(MIN_SWAP),
		3 => effective_cap.saturating_add(MIN_SWAP),
		4 => effective_cap / 2,
		5 => effective_cap.saturating_sub(1).max(MIN_SWAP),
		6 => effective_cap.saturating_add(1),
		_ => rng.gen_range(MIN_SWAP..=effective_cap),
	}
}

// ---------------------------------------------------------------------------
// Specific generators (domain knowledge)
// ---------------------------------------------------------------------------

fn gen_mint(rng: &mut StdRng, state: &FuzzState) -> Command {
	if state.assets.is_empty() {
		return Command::Mint { account: 1, asset_id: USDC_ASSET_ID, amount: MIN_SWAP };
	}
	let idx = pick_asset_weighted(rng, state, |a| a.remaining_ceiling);
	let asset = &state.assets[idx];
	let (account, balance) = pick_richest_account(rng, &asset.account_external);
	let amount = pick_mint_amount(rng, asset, balance, state);
	Command::Mint { account, asset_id: asset.asset_id, amount }
}

fn gen_redeem(rng: &mut StdRng, state: &FuzzState) -> Command {
	if state.assets.is_empty() {
		return Command::Redeem { account: 1, asset_id: USDC_ASSET_ID, amount: MIN_SWAP };
	}
	let idx = pick_asset_weighted(rng, state, |a| a.debt);
	let asset = &state.assets[idx];
	let (account, pusd_balance) = pick_richest_account(rng, &asset.account_pusd);
	let amount = pick_redeem_amount(rng, asset, pusd_balance);
	Command::Redeem { account, asset_id: asset.asset_id, amount }
}

// The most interesting generator: deliberately lowers max_psm_debt below the current
// total_psm_debt, creating a transient invariant violation. The ratio is computed as a
// random fraction (10-90%) of the debt-to-issuance ratio, guaranteeing the new cap is
// strictly below the outstanding debt. Subsequent operations must handle this state
// correctly — mints should be blocked, and the violation should persist until redeemed.
fn gen_lower_ceiling_below_debt(rng: &mut StdRng, state: &FuzzState) -> Command {
	let debt = state.total_psm_debt;
	let max_issuance = state.max_issuance;
	// max_issuance > 0: guarded by debt > 0 and debt <= max_psm_debt <= ratio * max_issuance; qed
	let max_ratio = Permill::from_rational(debt, max_issuance.max(1));
	let factor = Permill::from_percent(rng.gen_range(10..90));
	// Permill * Permill: multiply raw parts then divide by 1M
	let max_raw = max_ratio.deconstruct() as u128;
	let factor_raw = factor.deconstruct() as u128;
	let target_raw = max_raw * factor_raw / 1_000_000;
	let ratio = Permill::from_parts(target_raw.min(1_000_000) as u32);
	Command::SetMaxPsmDebt { ratio }
}

fn gen_raise_ceiling(rng: &mut StdRng, state: &FuzzState) -> Command {
	let current_parts = state.max_psm_debt_ratio.deconstruct() as u128;
	let increment = rng.gen_range(1..500_000) as u128;
	let new_parts = (current_parts + increment).min(1_000_000);
	Command::SetMaxPsmDebt { ratio: Permill::from_parts(new_parts as u32) }
}

// 50/50 split in weight assignment: half the time the new weight is set below what is
// needed for the current debt (creating a per-asset ceiling violation), and half the
// time it is set to a random value. The ceiling-violation path is the most interesting
// because it exercises the pallet's handling of debt that exceeds its allocated ceiling.
fn gen_redistribute_weights(rng: &mut StdRng, state: &FuzzState) -> Command {
	let debt_assets: Vec<&AssetState> = state.assets.iter().filter(|a| a.debt > 0).collect();
	// Non-empty guaranteed by guard condition; qed
	let asset = debt_assets.choose(rng).expect("at least one asset with debt");

	let new_weight = if state.max_psm_debt > 0 {
		let debt_ratio = Permill::from_rational(asset.debt, state.max_psm_debt);
		if rng.gen_bool(0.5) {
			// Set weight below what's needed for current debt
			Permill::from_parts((debt_ratio.deconstruct() as u128 / 2).min(1_000_000) as u32)
		} else {
			Permill::from_parts(rng.gen_range(1..=1_000_000))
		}
	} else {
		Permill::from_parts(rng.gen_range(1..=1_000_000))
	};

	Command::SetAssetCeilingWeight { asset_id: asset.asset_id, weight: new_weight }
}

fn gen_toggle_breaker_with_debt(rng: &mut StdRng, state: &FuzzState) -> Command {
	let debt_assets: Vec<&AssetState> = state.assets.iter().filter(|a| a.debt > 0).collect();
	// Non-empty guaranteed by guard condition; qed
	let asset = debt_assets.choose(rng).expect("at least one");

	let status = match rng.gen_range(0..=2) {
		0 => CircuitBreakerLevel::MintingDisabled, // most interesting: blocks new debt
		1 => CircuitBreakerLevel::AllDisabled,
		_ => CircuitBreakerLevel::AllEnabled, // re-enable
	};

	Command::SetAssetStatus { asset_id: asset.asset_id, status }
}

// The 100% fee variant is important because the mint still succeeds and increments debt,
// but the user receives zero pUSD output. This exercises the pallet's fee-application
// logic at its extreme — the debt accounting must remain correct even when the entire
// mint amount is absorbed by the fee.
fn gen_extreme_fee(rng: &mut StdRng, state: &FuzzState) -> Command {
	if state.assets.is_empty() {
		return Command::SetMintingFee { asset_id: USDC_ASSET_ID, fee: Permill::zero() };
	}
	// Non-empty verified above; qed
	let asset = state.assets.choose(rng).expect("at least one asset");
	let fee = match rng.gen_range(0..=4) {
		0 => Permill::zero(),              // no fee
		1 => Permill::from_percent(1),     // 1%
		2 => Permill::from_percent(50),    // 50%
		3 => Permill::from_parts(999_900), // 99.99%
		_ => Permill::one(),               // 100% — zero output, still counts
	};

	if rng.gen_bool(0.5) {
		Command::SetMintingFee { asset_id: asset.asset_id, fee }
	} else {
		Command::SetRedemptionFee { asset_id: asset.asset_id, fee }
	}
}

fn gen_add_asset_with_weight(rng: &mut StdRng, state: &FuzzState) -> Command {
	// Non-empty guaranteed by guard condition; qed
	let &asset_id = state.unapproved.choose(rng).expect("at least one unapproved");
	let weight = Permill::from_parts(rng.gen_range(1..=1_000_000));
	Command::AddExternalAsset { asset_id, weight }
}

fn gen_remove_zero_debt_asset(rng: &mut StdRng, state: &FuzzState) -> Command {
	let zero_debt: Vec<&AssetState> = state.assets.iter().filter(|a| a.debt == 0).collect();
	// Non-empty guaranteed by guard condition; qed
	let asset = zero_debt.choose(rng).expect("at least one zero-debt asset");
	Command::RemoveExternalAsset { asset_id: asset.asset_id }
}

// ---------------------------------------------------------------------------
// State-aware command generation with weighted selection
// ---------------------------------------------------------------------------

// Dynamic candidate list: generators are only added when their preconditions are met.
// For example, "lower ceiling below debt" is only offered when total_psm_debt > 0,
// and "add external asset" only when unapproved assets exist. This avoids wasting
// commands on operations that would trivially fail due to unsatisfied preconditions,
// directing entropy toward state-changing operations instead.
fn gen_command(rng: &mut StdRng, state: &FuzzState) -> Command {
	let mut candidates: Vec<(u32, fn(&mut StdRng, &FuzzState) -> Command)> = Vec::new();

	// Mint: interesting if any asset has remaining ceiling AND any account has balance
	if state.any_asset_can_mint() {
		candidates.push((20, gen_mint));
	}

	// Redeem: interesting if any asset has debt AND any account has pUSD
	if state.any_asset_can_redeem() {
		candidates.push((20, gen_redeem));
	}

	// Lower ceiling below current debt: HIGH interestingness — tests transient violations
	if state.total_psm_debt > 0 && state.max_psm_debt > 0 {
		candidates.push((8, gen_lower_ceiling_below_debt));
	}

	// Raise ceiling: interesting if debt is near ceiling
	if state.any_asset_near_ceiling() {
		candidates.push((5, gen_raise_ceiling));
	}

	// Change weights while debt exists: redistributes ceilings
	if state.assets.iter().any(|a| a.debt > 0 && a.weight > Permill::zero()) {
		candidates.push((8, gen_redistribute_weights));
	}

	// Toggle circuit breaker while debt exists
	if state.assets.iter().any(|a| a.debt > 0) {
		candidates.push((6, gen_toggle_breaker_with_debt));
	}

	// Set fee to extreme values (0%, 1%, 50%, 99.99%, 100%)
	candidates.push((4, gen_extreme_fee));

	// Add external asset (if any unapproved exist)
	if !state.unapproved.is_empty() {
		candidates.push((2, gen_add_asset_with_weight));
	}

	// Remove asset with zero debt
	if state.assets.iter().any(|a| a.debt == 0) {
		candidates.push((2, gen_remove_zero_debt_asset));
	}

	// Always have at least one candidate
	if candidates.is_empty() {
		candidates.push((1, gen_mint));
	}

	// Weighted random selection
	let total: u32 = candidates.iter().map(|(w, _)| *w).sum();
	if total == 0 {
		return gen_mint(rng, state);
	}
	let mut pick = rng.gen_range(0..total);
	for (weight, gen_fn) in &candidates {
		if pick < *weight {
			return gen_fn(rng, state);
		}
		pick -= weight;
	}
	(candidates[0].1)(rng, state)
}

// ---------------------------------------------------------------------------
// Command execution (dispatches via Psm::*, ignores expected errors)
// ---------------------------------------------------------------------------

fn execute_command(cmd: &Command) {
	match cmd {
		Command::Mint { account, asset_id, amount } => {
			if fh::is_approved_asset(*asset_id) {
				let _ = Psm::mint(RuntimeOrigin::signed(*account), *asset_id, *amount);
			}
		},
		Command::Redeem { account, asset_id, amount } => {
			if fh::is_approved_asset(*asset_id) {
				let _ = Psm::redeem(RuntimeOrigin::signed(*account), *asset_id, *amount);
			}
		},
		Command::SetMaxPsmDebt { ratio } => {
			let _ = Psm::set_max_psm_debt(RuntimeOrigin::root(), *ratio);
		},
		Command::SetAssetCeilingWeight { asset_id, weight } => {
			if fh::is_approved_asset(*asset_id) {
				let _ = Psm::set_asset_ceiling_weight(RuntimeOrigin::root(), *asset_id, *weight);
			}
		},
		Command::SetMintingFee { asset_id, fee } => {
			if fh::is_approved_asset(*asset_id) {
				let _ = Psm::set_minting_fee(RuntimeOrigin::root(), *asset_id, *fee);
			}
		},
		Command::SetRedemptionFee { asset_id, fee } => {
			if fh::is_approved_asset(*asset_id) {
				let _ = Psm::set_redemption_fee(RuntimeOrigin::root(), *asset_id, *fee);
			}
		},
		Command::SetAssetStatus { asset_id, status } => {
			if fh::is_approved_asset(*asset_id) {
				let _ = Psm::set_asset_status(RuntimeOrigin::root(), *asset_id, *status);
			}
		},
		// Paired add+weight: mirrors the libFuzzer target's approach. Without setting
		// a non-zero ceiling weight immediately after adding, the new asset would have
		// zero ceiling and all subsequent mints would fail trivially.
		Command::AddExternalAsset { asset_id, weight } => {
			if Psm::add_external_asset(RuntimeOrigin::root(), *asset_id).is_ok() {
				let _ = Psm::set_asset_ceiling_weight(RuntimeOrigin::root(), *asset_id, *weight);
			}
		},
		Command::RemoveExternalAsset { asset_id } => {
			let _ = Psm::remove_external_asset(RuntimeOrigin::root(), *asset_id);
		},
	}
}

// ---------------------------------------------------------------------------
// Logging helpers
// ---------------------------------------------------------------------------

fn asset_name(id: u32) -> &'static str {
	match id {
		USDC_ASSET_ID => "USDC",
		USDT_ASSET_ID => "USDT",
		DAI_ASSET_ID => "DAI",
		USDP_ASSET_ID => "USDP",
		FRAX_ASSET_ID => "FRAX",
		_ => "???",
	}
}

fn format_amount(raw: u128) -> String {
	let tokens = raw / PUSD_UNIT;
	if tokens >= 1_000_000 {
		format!("{:.1}M", tokens as f64 / 1_000_000.0)
	} else if tokens >= 1_000 {
		format!("{:.1}K", tokens as f64 / 1_000.0)
	} else {
		format!("{}", tokens)
	}
}

fn format_command(cmd: &Command) -> String {
	match cmd {
		Command::Mint { account, asset_id, amount } => {
			format!("Mint(acct={}, {}, {})", account, asset_name(*asset_id), amount)
		},
		Command::Redeem { account, asset_id, amount } => {
			format!("Redeem(acct={}, {}, {})", account, asset_name(*asset_id), amount)
		},
		Command::SetMaxPsmDebt { ratio } => {
			format!("SetMaxPsmDebt({:.3}%)", ratio.deconstruct() as f64 / 10_000.0)
		},
		Command::SetAssetCeilingWeight { asset_id, weight } => format!(
			"SetWeight({}, {:.3}%)",
			asset_name(*asset_id),
			weight.deconstruct() as f64 / 10_000.0
		),
		Command::SetMintingFee { asset_id, fee } => format!(
			"SetMintFee({}, {:.3}%)",
			asset_name(*asset_id),
			fee.deconstruct() as f64 / 10_000.0
		),
		Command::SetRedemptionFee { asset_id, fee } => format!(
			"SetRedeemFee({}, {:.3}%)",
			asset_name(*asset_id),
			fee.deconstruct() as f64 / 10_000.0
		),
		Command::SetAssetStatus { asset_id, status } => {
			format!("SetStatus({}, {:?})", asset_name(*asset_id), status)
		},
		Command::AddExternalAsset { asset_id, weight } => format!(
			"AddAsset({}, w={:.3}%)",
			asset_name(*asset_id),
			weight.deconstruct() as f64 / 10_000.0
		),
		Command::RemoveExternalAsset { asset_id } => {
			format!("RemoveAsset({})", asset_name(*asset_id))
		},
	}
}

fn log_command(step: usize, cmd: &Command, state: &FuzzState) {
	let total_reserve: u128 = state.assets.iter().map(|a| a.reserve).sum();
	eprintln!(
		"[{:>4}] {} | debt={}/{} issuance={}/{} reserve={}",
		step,
		format_command(cmd),
		format_amount(state.total_psm_debt),
		format_amount(state.max_psm_debt),
		format_amount(state.total_pusd_issuance),
		format_amount(state.max_issuance),
		format_amount(total_reserve),
	);
}

// ---------------------------------------------------------------------------
// Campaign runner
// ---------------------------------------------------------------------------

// Per-command try_state is stricter than the libFuzzer target's per-block check. Since
// this tester generates semantically aware commands (not raw fuzz bytes), it can assert
// invariants after every single call. A violation here means a specific command sequence
// broke a pallet invariant, and the deterministic seed makes it reproducible.
fn run_campaign(seed: u64, max_commands: usize) {
	let mut ext = build_fuzzer_genesis();
	let mut rng = StdRng::seed_from_u64(seed);
	let mut block_number: u32 = 1;

	// Random block advancement interval
	let mut next_block_at: usize = rng.gen_range(10..50);

	ext.execute_with(|| {
		for i in 0..max_commands {
			// Multi-block: advance block number periodically
			if i > 0 && i == next_block_at {
				block_number += 1;
				System::set_block_number(block_number.into());
				next_block_at = i + rng.gen_range(10..50);
			}

			let state = snapshot_state();
			let cmd = gen_command(&mut rng, &state);
			log_command(i, &cmd, &state);
			execute_command(&cmd);
			fh::do_try_state().expect("PSM invariant violated — see command log above");
		}
	});
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
	let seed: u64 = env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or_else(rand::random);
	let max_commands: usize = env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(10_000);

	eprintln!("PSM stateful tester: seed={}, max_commands={}", seed, max_commands);
	run_campaign(seed, max_commands);
	eprintln!("Campaign complete: {} commands, 0 invariant violations", max_commands);
}
