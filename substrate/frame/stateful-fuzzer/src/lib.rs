//! Stateful fuzzer for FRAME pallets.
//!
//! Two invariant kinds: **State** (single-snapshot, typically `do_try_state`) and **Trajectory**
//! (accumulator-based checks over the event stream).

use rand::{rngs::StdRng, Rng, SeedableRng};
use sp_io::TestExternalities;
use sp_runtime::TryRuntimeError;
use std::fmt;

pub struct Command<E> {
	pub weight: u32,
	pub label: &'static str,
	pub run: Box<dyn FnOnce() -> (Result<(), sp_runtime::DispatchError>, Vec<E>)>,
}

pub trait ErasedTraceChecker<E> {
	fn label(&self) -> &'static str;
	fn update(&mut self, events: &[E]);
	fn check(&self) -> Result<(), String>;
}

pub struct TraceChecker<A, E> {
	pub label: &'static str,
	pub init: A,
	pub update: fn(&mut A, &[E]),
	pub check: fn(&A) -> Result<(), String>,
}

struct TypedTraceChecker<A, E> {
	label: &'static str,
	acc: A,
	update_fn: fn(&mut A, &[E]),
	check_fn: fn(&A) -> Result<(), String>,
}

impl<A: 'static, E: 'static> ErasedTraceChecker<E> for TypedTraceChecker<A, E> {
	fn label(&self) -> &'static str {
		self.label
	}
	fn update(&mut self, events: &[E]) {
		(self.update_fn)(&mut self.acc, events);
	}
	fn check(&self) -> Result<(), String> {
		(self.check_fn)(&self.acc)
	}
}

impl<A: 'static, E: 'static> TraceChecker<A, E> {
	pub fn erased(self) -> Box<dyn ErasedTraceChecker<E>> {
		Box::new(TypedTraceChecker {
			label: self.label,
			acc: self.init,
			update_fn: self.update,
			check_fn: self.check,
		})
	}
}

pub struct Config<E> {
	pub genesis: fn() -> TestExternalities,
	pub commands: fn(&mut StdRng) -> Vec<Command<E>>,
	pub state_check: fn() -> Result<(), TryRuntimeError>,
	pub trace_checkers: Vec<Box<dyn ErasedTraceChecker<E>>>,
	pub advance_block: fn(u64),
	pub format_state: fn() -> String,
}

pub struct Stats {
	pub commands_run: usize,
	pub state_violations: usize,
	pub trace_violations: usize,
}

impl fmt::Display for Stats {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(
			f,
			"{} commands, {} state violations, {} trace violations",
			self.commands_run, self.state_violations, self.trace_violations
		)
	}
}

pub fn run<E: 'static>(
	config: &mut Config<E>,
	seed: u64,
	max_commands: usize,
	verbose: bool,
) -> Stats {
	let mut ext = (config.genesis)();
	let mut rng = StdRng::seed_from_u64(seed);
	let mut stats = Stats { commands_run: 0, state_violations: 0, trace_violations: 0 };
	let mut next_block_at: usize = rng.gen_range(10..50);
	let mut block: u64 = 1;

	ext.execute_with(|| {
		for i in 0..max_commands {
			if i > 0 && i == next_block_at {
				block += 1;
				(config.advance_block)(block);
				next_block_at = i + rng.gen_range(10..50);
			}

			let candidates = (config.commands)(&mut rng);
			if candidates.is_empty() {
				continue;
			}

			let total_weight: u32 = candidates.iter().map(|c| c.weight).sum();
			if total_weight == 0 {
				continue;
			}
			let mut pick = rng.gen_range(0..total_weight);
			let mut chosen_idx = 0;
			for (idx, cmd) in candidates.iter().enumerate() {
				if pick < cmd.weight {
					chosen_idx = idx;
					break;
				}
				pick -= cmd.weight;
			}

			let mut chosen: Option<Command<E>> = None;
			for (idx, cmd) in candidates.into_iter().enumerate() {
				if idx == chosen_idx {
					chosen = Some(cmd);
				}
			}
			let cmd = chosen.expect("chosen_idx is valid; qed");
			let label = cmd.label;
			let (result, events) = (cmd.run)();

			if verbose {
				let status = match &result {
					Ok(()) => "\x1b[32mOK\x1b[0m ",
					Err(_) => "\x1b[31mERR\x1b[0m",
				};
				eprintln!(
					"\x1b[2m[{:>5}]\x1b[0m {:<36} {} {}",
					i,
					label,
					status,
					(config.format_state)()
				);
			}

			for tc in config.trace_checkers.iter_mut() {
				tc.update(&events);
				if let Err(msg) = tc.check() {
					stats.trace_violations += 1;
					panic!(
						"Trace violation at step {} after {}: checker '{}': {}",
						i,
						label,
						tc.label(),
						msg
					);
				}
			}

			if let Err(e) = (config.state_check)() {
				stats.state_violations += 1;
				panic!("State violation at step {} after {}: {:?}", i, label, e);
			}

			stats.commands_run += 1;
		}
	});

	stats
}
