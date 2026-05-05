use arbitrary::{Arbitrary, Unstructured};
use std::{env, fs, io::Write, path::Path};

include!("psm_impl.rs");

// Converts a cumulative Op variant threshold into the minimum u32 selector value.
//
// Op::arbitrary draws a u32 then computes:
//   threshold = (rand_value * 51) / u32::MAX   (divides by MAX, not MAX+1)
// Cumulative thresholds: Mint=0, Redeem=15, SetMaxPsmDebt=30, SetCeiling=35,
//   SetMintFee=40, SetRedeemFee=43, SetStatus=46, AddAsset=49, RemoveAsset=50.
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

// AmountTier byte 0x00 → int_in_range(0..=4) % 5 = 0 → MinSwap.
// MinSwap = 100 * INTERNAL_UNIT = 1e8. For DAI (18 dec): 1e8 / 10^(18-6) = 0 → error.
const TIER_MIN_SWAP: u8 = 0;

// asset_idx % 4 indexes ALL_EXTERNAL_ASSETS = [USDC, USDT, USDX, DAI].
const USDC_IDX: u8 = 0; // pre-approved in PSM genesis
const DAI_IDX: u8 = 3; // 18 decimals, not pre-approved

fn seed_already_approved() -> Vec<u8> {
	let mut v = vec![multiblock_byte(1), block_byte(1)];
	v.extend_from_slice(&op_selector(49));
	v.push(USDC_IDX);
	v
}

fn seed_amount_too_small() -> Vec<u8> {
	let mut v = vec![multiblock_byte(2), block_byte(1)];
	v.extend_from_slice(&op_selector(49));
	v.push(DAI_IDX);
	v.push(block_byte(1));
	v.extend_from_slice(&op_selector(0));
	v.extend_from_slice(&[0x00, DAI_IDX, TIER_MIN_SWAP]);
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
		("seed_already_approved", seed_already_approved()),
		("seed_amount_too_small", seed_amount_too_small()),
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
