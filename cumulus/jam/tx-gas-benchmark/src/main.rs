// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! The refine gas parachain transactions cost on JAM, measured offline and in memory.
//!
//! Builds two parachain blocks on the same parent the way the JAM collator builds them, one
//! without transactions and one with `--transactions` signed `transfer_keep_alive`s, then runs
//! `jam_validate_block` on each PoV the way the Parachain Service's refine runs it and reports the
//! gas of the inner PVM. See the README for how to build the inputs.

mod block;
mod jam_state;
mod profile;
mod refine;

use clap::Parser;
use codec::Encode;
use std::{path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(about)]
struct Cli {
	/// The parachain runtime: `parachain-template-runtime` built for PolkaVM with `--cfg jam`.
	#[arg(long)]
	runtime: PathBuf,
	/// The block authorizer: the parachain service's `parachain-authorizer-sr25519.jam`.
	#[arg(long)]
	authorizer: PathBuf,
	/// Transactions in the measured block.
	#[arg(long, default_value_t = 50)]
	transactions: u32,
	/// Attribute the refine gas instruction by instruction and write the result under this
	/// directory (per function, per call stack, per program counter).
	#[arg(long)]
	profile: Option<PathBuf>,
}

fn main() -> ExitCode {
	match run(Cli::parse()) {
		Ok(()) => ExitCode::SUCCESS,
		Err(error) => {
			eprintln!("error: {error}");
			ExitCode::FAILURE
		},
	}
}

fn run(cli: Cli) -> Result<(), String> {
	// Block building runs the runtime through the node's executor, which only runs PolkaVM
	// runtimes when asked to. The interpreter needs no userfaultfd. The refine run below uses its
	// own polkavm and is not affected by these.
	for (key, value) in [
		("SUBSTRATE_ENABLE_POLKAVM", "1"),
		("POLKAVM_BACKEND", "interpreter"),
		("POLKAVM_ALLOW_INSECURE", "1"),
	] {
		if std::env::var_os(key).is_none() {
			std::env::set_var(key, value);
		}
	}
	let read = |path: &PathBuf| {
		std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))
	};
	let runtime = read(&cli.runtime)?;
	let authorizer = read(&cli.authorizer)?;
	let pvf = refine::Pvf::parse(&runtime)?;

	let mut chain = block::Chain::new(&runtime, &authorizer)?;
	// The measured blocks build on block 1, so neither of them pays for being the first block.
	let first = chain.build(0)?;
	chain.import(first)?;
	let empty = chain.build(0)?;
	let full = chain.build(cli.transactions)?;

	let empty_gas = measure(&pvf, &empty, cli.profile.as_deref().map(|_| "empty"))?;
	let full_gas = measure(&pvf, &full, cli.profile.as_deref().map(|_| "full"))?;
	if cli.transactions > 0 {
		println!(
			"per transaction: {} gas",
			group((full_gas - empty_gas) / i64::from(cli.transactions))
		);
	}
	Ok(())
}

/// Refine `block` and check that the runtime validated it, returning the inner PVM's gas.
fn measure(
	pvf: &refine::Pvf,
	block: &block::BuiltBlock,
	profile: Option<&str>,
) -> Result<i64, String> {
	let what = format!("block #{} with {} transactions", block.number, block.transactions);
	let outcome = match profile {
		Some(label) => refine::run_profiled(pvf, &block.pov, &block.refine_context, label),
		None => refine::run(pvf, &block.pov, &block.refine_context),
	}
	.map_err(|error| format!("refine of {what} failed: {error}"))?;
	// The service stores a block's hash as its head and binds the parent through the hash of the
	// encoded parent hash.
	if outcome.head.as_deref() != Some(&block.hash.encode()[..]) {
		return Err(format!("refine of {what} declared head {:?}", outcome.head));
	}
	let parent_head_hash = parachain_service_core::blake2_256(&block.parent_hash.encode());
	if outcome.parent_head_hash != Some(parent_head_hash) {
		return Err(format!(
			"refine of {what} declared parent head hash {:?}",
			outcome.parent_head_hash
		));
	}
	println!(
		"{what}: {} gas (PoV {} bytes, {} host calls, {} upward messages)",
		group(outcome.gas),
		group(block.pov.len() as i64),
		outcome.host_calls,
		outcome.upward_messages,
	);
	Ok(outcome.gas)
}

/// `1234567` as `1,234,567`.
fn group(value: i64) -> String {
	let digits = value.unsigned_abs().to_string();
	let mut grouped = String::new();
	// A separator goes before every digit that has a multiple of three digits after it.
	let lead = digits.len() % 3;
	for (index, digit) in digits.chars().enumerate() {
		if index > 0 && index % 3 == lead {
			grouped.push(',');
		}
		grouped.push(digit);
	}
	if value < 0 {
		grouped.insert(0, '-');
	}
	grouped
}
