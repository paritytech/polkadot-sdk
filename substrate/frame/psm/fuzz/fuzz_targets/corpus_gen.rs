use arbitrary::{Arbitrary, Unstructured};
use pallet_psm_fuzz::MultiBlockOps;
use std::{env, fs, io::Write, path::Path};

// Converts a cumulative Op variant threshold into the minimum u32 selector value.
//
// Op::arbitrary draws a u32 then computes:
//   threshold = (rand_value * total_weight) / u32::MAX   (divides by MAX, not MAX+1)
//   then iterates CALL_SPECS subtracting each weight until threshold < spec.weight.
// Weights: Mint=15, Redeem=15, SetMaxPsmDebt=5, SetCeiling=5,
//   SetMintFee=3, SetRedeemFee=3, SetStatus=3, AddAsset=1, RemoveAsset=1. Total=51.
// Cumulative thresholds: Mint=0, Redeem=15, SetMaxDebt=30, SetCeiling=35,
//   SetMintFee=40, SetRedeemFee=43, SetStatus=46, AddAsset=49, RemoveAsset=50.
//
// dispatch_op pre-filters AddExternalAsset to non-approved assets only; USDC and USDT
// are always approved in genesis, so AssetAlreadyApproved (line 919) is unreachable.
fn op_selector(cumulative_threshold: u64) -> [u8; 4] {
	if cumulative_threshold == 0 {
		return [0u8; 4];
	}
	let rand_value = ((cumulative_threshold * u32::MAX as u64) + 50) / 51;
	(rand_value as u32).to_le_bytes()
}

// MultiBlockOps byte: tier = byte & 0x1F, variation = byte >> 5.
// tier 0-9: num_blocks = 1 + (variation % 5). Uses tier=0, variation=num_blocks-1.
fn multiblock_byte(num_blocks: usize) -> u8 {
	((num_blocks - 1) as u8) << 5
}

// Block byte: tier = byte & 0x1F, variation = byte >> 5.
// tier 0-15: num_calls = 1 + (variation % 3). Uses tier=0, variation=num_ops-1.
fn block_byte(num_ops: usize) -> u8 {
	((num_ops - 1) as u8) << 5
}

// AmountTier byte 0x00 → int_in_range(0..=4) % 5 = 0 → MinSwap (100 * INTERNAL_UNIT = 1e8).
const TIER_MIN_SWAP: u8 = 0;

// asset_idx for AddExternalAsset: candidates = non-approved assets = [USDX, DAI_MOCK].
// asset_idx % 2: 0 → USDX (2 dec), 1 → DAI_MOCK (18 dec).
const ADD_ASSET_USDX: u8 = 0;
const ADD_ASSET_DAI: u8 = 1;

// asset_idx for Mint/Redeem: ALL_EXTERNAL_ASSETS = [USDC, USDT, USDX, DAI_MOCK].
// asset_idx % 4: 0 → USDC, 1 → USDT, 2 → USDX, 3 → DAI_MOCK.
const ASSET_USDC: u8 = 0;
const ASSET_USDT: u8 = 1;
const ASSET_USDX: u8 = 2;
const ASSET_DAI: u8 = 3;

// Seed 1: AmountTooSmallAfterConversion in mint (lib.rs line 541).
// AddExternalAsset(DAI) + Mint(DAI, MinSwap): 1e8 DAI wei / 10^(18-6) = 0 pUSD → error.
fn seed_mint_too_small_dai() -> Vec<u8> {
	let mut v = vec![multiblock_byte(2), block_byte(1)];
	v.extend_from_slice(&op_selector(49));
	v.push(ADD_ASSET_DAI);
	v.push(block_byte(1));
	v.extend_from_slice(&op_selector(0));
	v.extend_from_slice(&[0x00, ASSET_DAI, TIER_MIN_SWAP]);
	v
}

// Seed 2: AmountTooSmallAfterConversion in redeem (lib.rs line 666).
// SetRedemptionFee(USDX, 999_999 / 1_000_000 ≈ 100%) so that after fee,
// internal_net ≈ 100 pUSD → external_out = 100 / 10^(6-2) = 0 USDX → error.
// Sequence: AddExternalAsset(USDX), Mint(USDC, MinSwap) to get pUSD,
//           SetRedemptionFee(USDX, 999_999), Redeem(USDX, MinSwap).
fn seed_redeem_too_small_usdx() -> Vec<u8> {
	let redeem_fee_sel = op_selector(43);
	let parts_999999: [u8; 4] = (999_999u32).to_le_bytes();
	let redeem_sel = op_selector(15);

	let mut v = vec![multiblock_byte(4)];
	v.push(block_byte(1));
	v.extend_from_slice(&op_selector(49));
	v.push(ADD_ASSET_USDX);
	v.push(block_byte(1));
	v.extend_from_slice(&op_selector(0));
	v.extend_from_slice(&[0x00, ASSET_USDC, TIER_MIN_SWAP]);
	v.push(block_byte(1));
	v.extend_from_slice(&redeem_fee_sel);
	v.push(ASSET_USDX);
	v.extend_from_slice(&parts_999999);
	v.push(block_byte(1));
	v.extend_from_slice(&redeem_sel);
	v.extend_from_slice(&[0x00, ASSET_USDX, TIER_MIN_SWAP]);
	v
}

// Seed 3: ExceedsMaxIssuance (lib.rs line 557).
// MaximumIssuance = 10M UNIT. Strategy: SetMaxPsmDebt(100%) → fill USDC (5M) + USDT (4M,
// ceiling-limited) + USDX (≥1M) → total > 10M → next mint fires ExceedsMaxIssuance.
// Requires 10 accounts × 2 USDC mints + 10 accounts × USDT mints + USDX setup.
// Each account has 500K tokens per asset = 500K UNIT pUSD equivalent.
// Simplified: AddExternalAsset(USDX), SetMaxPsmDebt(1_000_000 = 100%),
//             SetCeilingWeight(USDX, large), then mint from many accounts.
fn seed_exceeds_max_issuance() -> Vec<u8> {
	let set_max_debt_sel = op_selector(30);
	let set_ceiling_sel = op_selector(35);
	let mint_sel = op_selector(0);
	let parts_100pct: [u8; 4] = (1_000_000u32).to_le_bytes();
	let parts_300k: [u8; 4] = (300_000u32).to_le_bytes();
	const TIER_AT_CEILING: u8 = 2;

	let mut v = vec![multiblock_byte(5)];

	v.push(block_byte(1));
	v.extend_from_slice(&op_selector(49));
	v.push(ADD_ASSET_USDX);

	v.push(block_byte(2));
	v.extend_from_slice(&set_max_debt_sel);
	v.extend_from_slice(&parts_100pct);
	v.extend_from_slice(&set_ceiling_sel);
	v.push(ASSET_USDX);
	v.extend_from_slice(&parts_300k);

	v.push(block_byte(3));
	for acct in [0u8, 1, 2] {
		v.extend_from_slice(&mint_sel);
		v.extend_from_slice(&[acct, ASSET_USDC, TIER_AT_CEILING]);
	}

	v.push(block_byte(3));
	for acct in [3u8, 4] {
		v.extend_from_slice(&mint_sel);
		v.extend_from_slice(&[acct, ASSET_USDC, TIER_AT_CEILING]);
	}
	v.extend_from_slice(&mint_sel);
	v.extend_from_slice(&[0x00, ASSET_USDT, TIER_AT_CEILING]);

	v.push(block_byte(3));
	for acct in [1u8, 2] {
		v.extend_from_slice(&mint_sel);
		v.extend_from_slice(&[acct, ASSET_USDT, TIER_AT_CEILING]);
	}
	v.extend_from_slice(&mint_sel);
	v.extend_from_slice(&[0x00, ASSET_USDX, TIER_AT_CEILING]);

	v
}

fn verify(name: &str, data: &[u8]) -> bool {
	let mut u = Unstructured::new(data);
	match MultiBlockOps::arbitrary(&mut u) {
		Ok(ops) => {
			let n_ops: usize = ops.0.iter().map(|b| b.0.len()).sum();
			eprintln!(
				"  {} ({} bytes): {} blocks, {} ops — OK",
				name,
				data.len(),
				ops.0.len(),
				n_ops
			);
			true
		},
		Err(e) => {
			eprintln!("  {} FAILED: {:?}", name, e);
			false
		},
	}
}

fn main() {
	let out = env::args().nth(1).unwrap_or_else(|| "corpus/psm".to_string());
	let dir = Path::new(&out);
	fs::create_dir_all(dir).expect("cannot create corpus dir");

	let seeds: &[(&str, Vec<u8>)] = &[
		("seed_mint_too_small_dai", seed_mint_too_small_dai()),
		("seed_redeem_too_small_usdx", seed_redeem_too_small_usdx()),
		("seed_exceeds_max_issuance", seed_exceeds_max_issuance()),
	];

	let mut ok = true;
	for (name, bytes) in seeds {
		if !verify(name, bytes) {
			ok = false;
			continue;
		}
		let mut f = fs::File::create(dir.join(name)).expect("cannot create seed file");
		f.write_all(bytes).expect("cannot write seed");
		eprintln!("  written: {}", dir.join(name).display());
	}

	if !ok {
		std::process::exit(1);
	}
	eprintln!("Done — {} seeds written to {}", seeds.len(), out);
}
