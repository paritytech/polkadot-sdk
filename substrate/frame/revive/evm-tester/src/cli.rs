//! Command line interface for the statetest runner

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "statetest")]
#[command(about = "State test runner for Revive EVM compatibility testing")]
pub struct Args {
	/// Test file path, or read from stdin if not provided
	pub test_file: Option<PathBuf>,

	/// Only run tests for the specified fork
	#[arg(long = "statetest.fork")]
	pub fork: Option<String>,

	/// The index of the subtest to run (-1 for all)
	#[arg(long = "statetest.index", default_value = "-1")]
	pub index: i32,

	/// Run only tests matching the regular expression
	#[arg(long = "run", default_value = ".*")]
	pub run: String,

	/// Benchmark the execution
	#[arg(long = "bench")]
	pub bench: bool,

	/// Dump the state after the run
	#[arg(long = "dump")]
	pub dump: bool,

	/// Human-readable output
	#[arg(long = "human")]
	pub human: bool,
}