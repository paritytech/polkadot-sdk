//! State test runner for Revive EVM compatibility testing
//! 
//! This binary replicates the functionality of go-ethereum's `evm statetest` command
//! for validating EVM implementations against the official Ethereum test suite.

use anyhow::{Context, Result};
use clap::Parser;
use regex::Regex;
use std::{
	fs,
	io::{self, BufRead},
	path::PathBuf,
};

use revm_statetest_types::{
    TestSuite as StateTestSuite,
    SpecName,
};

mod cli;
mod executor;

#[cfg(test)]
mod tests;

use cli::Args;
use executor::{execute_state_test, report, TestResult};

fn main() -> Result<()> {
	let args = Args::parse();

	if let Some(test_file) = &args.test_file {
		// Single file mode
		let files = collect_files(test_file)?;
		let mut results = Vec::new();
		for file in files {
			let test_results = run_state_test(&args, &file)?;
			results.extend(test_results);
		}
		report(&args, results);
	} else {
		// Batch mode - read filenames from stdin
		let stdin = io::stdin();
		for line in stdin.lock().lines() {
			let filename = line?;
			if filename.is_empty() {
				break;
			}
			let results = run_state_test(&args, &PathBuf::from(filename))?;
			report(&args, results);
		}
	}

	Ok(())
}

/// Collect files to process - if path is a file, return it; if directory, return all .json files
fn collect_files(path: &PathBuf) -> Result<Vec<PathBuf>> {
	if path.is_file() {
		Ok(vec![path.clone()])
	} else if path.is_dir() {
		let mut files = Vec::new();
		for entry in fs::read_dir(path)? {
			let entry = entry?;
			let path = entry.path();
			if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
				files.push(path);
			}
		}
		files.sort();
		Ok(files)
	} else {
		Ok(vec![path.clone()]) // Let it fail later with proper error
	}
}

/// Run state tests from a single file
fn run_state_test(args: &Args, file_path: &PathBuf) -> Result<Vec<TestResult>> {
	let content = fs::read_to_string(file_path)
		.with_context(|| format!("Failed to read test file: {:?}", file_path))?;

	let test_suite: StateTestSuite = serde_json::from_str(&content)
		.with_context(|| format!("Failed to parse test file: {:?}", file_path))?;

	let regex =
		Regex::new(&args.run).with_context(|| format!("Invalid regex pattern: {}", args.run))?;

	let mut results = Vec::new();

	for (test_name, test_case) in test_suite.0 {
		if !regex.is_match(&test_name) {
			continue;
		}

		// Process each fork in the post states
		for (fork, post_states) in &test_case.post {
			// Filter by fork if specified
			if let Some(target_fork) = &args.fork {
				if !fork_matches(fork, target_fork) {
					continue;
				}
			}

			// Process each subtest
			for (i, post_state) in post_states.iter().enumerate() {
				// Filter by index if specified
				if args.index >= 0 && i != args.index as usize {
					continue;
				}

				let result = execute_state_test(&test_name, &test_case, &format!("{:?}", fork), i, post_state, args)?;
				results.push(result);
			}
		}
	}

	Ok(results)
}


/// Check if a SpecName fork matches a target fork string
fn fork_matches(spec_fork: &SpecName, target_fork: &str) -> bool {
	// Convert SpecName to string for comparison
	let fork_str = format!("{:?}", spec_fork);
	fork_str == target_fork
}

// Helper functions removed - using revm types directly now
