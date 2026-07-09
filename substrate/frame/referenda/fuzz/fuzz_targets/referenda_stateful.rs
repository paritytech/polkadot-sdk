use pallet_referenda_fuzz::{mock, stateful};
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::env;

const BLUE: &str = "\x1b[44;97m";
const YELLOW: &str = "\x1b[43;30m";
const RED: &str = "\x1b[41;97m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

fn format_state(s: &stateful::FuzzState) -> String {
	format!(
		"blk={} refs={} t0:dec={}/q={} t1:dec={}/q={}",
		s.block, s.ref_count,
		s.track0_deciding, s.track0_queue_len,
		s.track1_deciding, s.track1_queue_len,
	)
}

fn run_campaign(seed: u64, max_commands: usize, verbose: bool) {
	let mut ext = mock::build_genesis();
	let mut rng = StdRng::seed_from_u64(seed);
	let infinite = max_commands == 0;
	let mut next_block_at: usize = rng.gen_range(10..50);

	ext.execute_with(|| {
		let mut i = 0usize;
		loop {
			if !infinite && i >= max_commands { break; }

			if i > 0 && i == next_block_at {
				mock::next_block();
				next_block_at = i + rng.gen_range(10..50);
			}

			let pre = stateful::snapshot_state();
			let cmd = stateful::gen_command(&mut rng, &pre);
			let result = stateful::execute_command(&cmd);
			let post = stateful::snapshot_state();
			let check = mock::do_try_state();

			let invariant_str = match &check {
				Ok(()) => "OK".to_string(),
				Err(e) => format!("{RED}{BOLD}VIOLATED: {:?}{RESET}", e),
			};

			if verbose {
				eprintln!(
					"[{:>6}] {:<42} dispatch={:<4} invariant={} {BLUE}PRE  {}{RESET} {YELLOW}POST {}{RESET}",
					i, stateful::format_command(&cmd), result, invariant_str,
					format_state(&pre), format_state(&post),
				);
			} else if check.is_err() {
				eprintln!(
					"[{:>6}] {} dispatch={} invariant={}\n         {BLUE}PRE  {}{RESET}\n         {YELLOW}POST {}{RESET}",
					i, stateful::format_command(&cmd), result, invariant_str,
					format_state(&pre), format_state(&post),
				);
			}

			if let Err(e) = check {
				panic!("Invariant violated at command {}: {:?}", i, e);
			}

			i += 1;
		}
	});
}

fn main() {
	let args: Vec<String> = env::args().collect();
	let verbose = args.iter().any(|a| a == "--verbose");
	let positional: Vec<&str> = args.iter().skip(1)
		.filter(|a| !a.starts_with('-'))
		.map(|s| s.as_str())
		.collect();

	let seed: u64 = positional.first().and_then(|s| s.parse().ok()).unwrap_or_else(rand::random);
	let max_commands: usize = positional.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);

	if max_commands == 0 {
		eprintln!("Referenda stateful tester: seed={}, running indefinitely (Ctrl+C to stop)", seed);
	} else {
		eprintln!("Referenda stateful tester: seed={}, max_commands={}", seed, max_commands);
	}

	run_campaign(seed, max_commands, verbose);

	if max_commands > 0 {
		eprintln!("Campaign complete: {} commands, 0 invariant violations", max_commands);
	}
}
