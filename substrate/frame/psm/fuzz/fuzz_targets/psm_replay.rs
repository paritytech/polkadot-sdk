// Corpus replay binary for source-level coverage measurement.
//
// Reads every file in the libFuzzer corpus directory, deserializes each as
// MultiBlockOps using the same Arbitrary instance as the libFuzzer target,
// and runs the identical block execution loop. Build with
// `-C instrument-coverage` to produce profraw data compatible with llvm-cov.
//
// Usage:
//   RUSTFLAGS="-C instrument-coverage" \
//   LLVM_PROFILE_FILE="/path/to/profraw/libfuzzer-%p.profraw" \
//   cargo build --bin psm_replay
//   ./target/debug/psm_replay [corpus_dir]

use std::env;
use std::fs;
use arbitrary::{Arbitrary, Unstructured};

include!("psm_impl.rs");

fn main() {
	let corpus_dir = env::args()
		.nth(1)
		.unwrap_or_else(|| "corpus/psm".to_string());

	let entries = fs::read_dir(&corpus_dir)
		.unwrap_or_else(|e| panic!("cannot read corpus dir {}: {}", corpus_dir, e));

	let mut replayed = 0usize;

	for entry in entries.flatten() {
		let path = entry.path();
		if !path.is_file() {
			continue;
		}
		let data = match fs::read(&path) {
			Ok(d) => d,
			Err(_) => continue,
		};

		let mut u = Unstructured::new(&data);
		let input = match MultiBlockOps::arbitrary(&mut u) {
			Ok(i) => i,
			Err(_) => continue,
		};

		let mut ext = build_fuzzer_genesis();
		let mut block_number: u32 = 1;

		for block in input.0.iter() {
			ext.execute_with(|| {
				System::set_block_number(block_number.into());
				for op in &block.0 {
					dispatch_op(op);
				}
				fuzz_helpers::do_try_state().expect("PSM invariant violated during corpus replay");
			});
			block_number += 1;
		}

		replayed += 1;
	}

	eprintln!("Replayed {} corpus files", replayed);
}
