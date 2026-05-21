#![no_main]

//! Coverage-guided fuzzer for pallet-psm.
//!
//! Generates multi-block sequences of PSM dispatchables, executes them with
//! state-aware amount generation, and validates invariants via `do_try_state`
//! after each block. Uses libFuzzer's coverage feedback to explore interesting
//! call sequences.

use libfuzzer_sys::fuzz_target;
use pallet_psm_fuzz::{build_fuzzer_genesis, dispatch_op, fuzz_helpers, MultiBlockOps, System};
use std::fs::OpenOptions;
use std::io::Write;

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
			fuzz_helpers::do_try_state().expect("PSM invariant violated");
		});
		block_number += 1;
	}
});
