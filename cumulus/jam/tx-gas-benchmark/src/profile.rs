// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! Where the inner PVM's gas goes, instruction by instruction.
//!
//! With polkavm's step tracing on, every executed instruction comes back as
//! [`InterruptKind::Step`]. Counting each program counter and joining the counts with the cost
//! model's per-basic-block gas (`Module::calculate_gas_cost_for`) attributes the run's gas to
//! code offsets; the runtime blob's own debug line programs then name the frames those offsets
//! belong to.
//!
//! A shadow call stack built from the call instructions (`load_imm_and_jump*` into `RA`) turns
//! the flat per-PC attribution into caller/callee stacks, so a cost inside a shared leaf such as
//! `memcpy` is charged to whoever called it.

use polkavm::{program::Instruction, Module, ProgramBlob, ProgramCounter, Reg};
use std::{
	collections::HashMap,
	fmt::Write as _,
	path::{Path, PathBuf},
};

/// One profiled run: per-PC step counts, plus the call-site tree they were reached through.
#[derive(Default)]
pub struct Profile {
	/// Executed instructions per program counter.
	counts: HashMap<u32, u64>,
	/// Call instruction PC -> the PC execution returns to.
	call_sites: HashMap<u32, u32>,
	/// Node 0 is the root; every other node is `(parent, call pc)`.
	nodes: Vec<(u32, u32)>,
	node_ids: HashMap<(u32, u32), u32>,
	/// `(return address, node)` of the calls in progress, innermost last.
	stack: Vec<(u32, u32)>,
	/// `(node << 32 | pc)` -> steps.
	stacked: HashMap<u64, u64>,
	steps: u64,
	/// Gas the run consumed, for the check that the attribution adds up.
	pub gas: i64,
	/// Steps that could not be attributed to a basic block.
	unattributed: u64,
}

impl Profile {
	pub fn new(code: &ProgramBlob) -> Self {
		Profile {
			call_sites: call_sites(code),
			nodes: vec![(u32::MAX, u32::MAX)],
			..Default::default()
		}
	}

	/// Record one executed instruction at `pc`.
	pub fn step(&mut self, pc: u32) {
		*self.counts.entry(pc).or_default() += 1;
		self.steps += 1;
		if self.stack.last().is_some_and(|&(ret, _)| ret == pc) {
			self.stack.pop();
		}
		let node = self.stack.last().map_or(0, |&(_, node)| node);
		*self.stacked.entry((node as u64) << 32 | pc as u64).or_default() += 1;
		if let Some(&ret) = self.call_sites.get(&pc) {
			let next = self.nodes.len() as u32;
			let child = *self.node_ids.entry((node, pc)).or_insert(next);
			if child == next {
				self.nodes.push((node, pc));
			}
			self.stack.push((ret, child));
		}
	}

	/// Gas of `pc`'s basic block times how often it ran, summed over every executed block.
	///
	/// `module` must be the module the run used, so its cost model is the one the run paid.
	pub fn gas_total(&mut self, module: &Module) -> i64 {
		let mut total = 0i64;
		for (&pc, &count) in &self.counts {
			match module.calculate_gas_cost_for(ProgramCounter(pc)) {
				Some(cost) => total += i64::from(cost) * count as i64,
				None => self.unattributed += count,
			}
		}
		total
	}

	/// Write the profile into `dir`: gas per function, gas per call stack, and raw per-PC counts.
	pub fn dump(
		&self,
		dir: &Path,
		module: &Module,
		symbols: &ProgramBlob,
		label: &str,
	) -> Result<(), String> {
		std::fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
		let mut gas_per_pc = Vec::with_capacity(self.counts.len());
		for (&pc, &count) in &self.counts {
			let gas = module
				.calculate_gas_cost_for(ProgramCounter(pc))
				.map(|cost| i64::from(cost) * count as i64)
				.unwrap_or(0);
			gas_per_pc.push((pc, count, gas));
		}
		gas_per_pc.sort_by_key(|&(_, _, gas)| -gas);

		let mut by_function: HashMap<String, i64> = HashMap::new();
		let mut by_stack: HashMap<String, i64> = HashMap::new();
		let mut frames: HashMap<u32, String> = HashMap::new();
		let mut node_frames: HashMap<u32, String> = HashMap::new();
		for &(pc, _, gas) in &gas_per_pc {
			let leaf = frames.entry(pc).or_insert_with(|| frames_at(symbols, pc)).clone();
			*by_function.entry(leaf.clone()).or_default() += gas;
		}
		for (&key, &count) in &self.stacked {
			let (node, pc) = ((key >> 32) as u32, key as u32);
			let gas = module
				.calculate_gas_cost_for(ProgramCounter(pc))
				.map(|cost| i64::from(cost) * count as i64)
				.unwrap_or(0);
			let prefix = node_frames
				.entry(node)
				.or_insert_with(|| {
					let mut chain = Vec::new();
					let mut node = node;
					while node != 0 {
						chain.push(node);
						node = self.nodes[node as usize].0;
					}
					chain.reverse();
					chain
						.iter()
						.map(|&node| {
							let call_pc = self.nodes[node as usize].1;
							frames
								.entry(call_pc)
								.or_insert_with(|| frames_at(symbols, call_pc))
								.clone()
						})
						.collect::<Vec<_>>()
						.join(";")
				})
				.clone();
			let leaf = frames.entry(pc).or_insert_with(|| frames_at(symbols, pc)).clone();
			let key = if prefix.is_empty() { leaf } else { format!("{prefix};{leaf}") };
			*by_stack.entry(key).or_default() += gas;
		}

		let total: i64 = by_function.values().sum();
		write_sorted(dir.join(format!("{label}.functions.tsv")), &by_function, total)?;
		write_sorted(dir.join(format!("{label}.stacks.tsv")), &by_stack, total)?;
		let mut raw = String::new();
		writeln!(raw, "pc\tsteps\tgas\tfunction").map_err(|error| error.to_string())?;
		for &(pc, count, gas) in &gas_per_pc {
			let name = frames.get(&pc).map(String::as_str).unwrap_or("<unknown>");
			writeln!(raw, "{pc}\t{count}\t{gas}\t{name}").map_err(|error| error.to_string())?;
		}
		std::fs::write(dir.join(format!("{label}.pc.tsv")), raw)
			.map_err(|error| error.to_string())?;
		println!(
			"{label}: {} steps, {} gas attributed, {} steps unattributed, {} distinct pcs, {} call nodes",
			self.steps,
			group(total),
			self.unattributed,
			self.counts.len(),
			self.nodes.len(),
		);
		Ok(())
	}
}

/// The directory profiles go to, when profiling was asked for.
pub fn dir() -> Option<PathBuf> {
	std::env::var_os("JAM_TX_GAS_PROFILE_DIR").map(PathBuf::from)
}

fn write_sorted(path: PathBuf, values: &HashMap<String, i64>, total: i64) -> Result<(), String> {
	let mut rows: Vec<(&String, i64)> = values.iter().map(|(k, v)| (k, *v)).collect();
	rows.sort_by_key(|&(_, gas)| -gas);
	let mut out = String::new();
	writeln!(out, "gas\tshare\tname").map_err(|error| error.to_string())?;
	for (name, gas) in rows {
		let share = if total == 0 { 0.0 } else { gas as f64 * 100.0 / total as f64 };
		writeln!(out, "{gas}\t{share:.2}%\t{name}").map_err(|error| error.to_string())?;
	}
	std::fs::write(path, out).map_err(|error| error.to_string())
}

/// The call instructions of `code`: PC -> return address.
fn call_sites(code: &ProgramBlob) -> HashMap<u32, u32> {
	code.instructions()
		.filter_map(|instruction| match instruction.kind {
			Instruction::load_imm_and_jump(ra, ..) |
			Instruction::load_imm_and_jump_indirect(ra, ..)
				if ra.get() == Reg::RA =>
			{
				Some((instruction.offset.0, instruction.next_offset.0))
			},
			_ => None,
		})
		.collect()
}

/// The innermost frame `pc` belongs to, as `crate::function` with generic hashes dropped.
fn frames_at(blob: &ProgramBlob, pc: u32) -> String {
	let Ok(Some(mut program)) = blob.get_debug_line_program_at(ProgramCounter(pc)) else {
		return format!("<pc {pc}>");
	};
	while let Ok(Some(region)) = program.run() {
		let range = region.instruction_range();
		if range.start.0 <= pc && pc < range.end.0 {
			let frames: Vec<String> = region
				.frames()
				.map(|frame| {
					frame
						.full_name()
						.map(|name| strip_hashes(&name.to_string()))
						.unwrap_or_default()
				})
				.collect();
			if let Some(name) = frames.last() {
				return name.clone();
			}
		}
	}
	format!("<pc {pc}>")
}

/// `foo[c0ffee]::bar::<x[y]>` as `foo::bar::<x>`: the hash in `[...]` dropped when it is one.
fn strip_hashes(name: &str) -> String {
	let mut out = String::with_capacity(name.len());
	let mut rest = name;
	while let Some(index) = rest.find('[') {
		out.push_str(&rest[..index]);
		let tail = &rest[index + 1..];
		match tail.find(']') {
			Some(end)
				if end > 0 && tail[..end].chars().all(|c| c.is_ascii_hexdigit() || c == ':') =>
			{
				rest = &tail[end + 1..]
			},
			_ => {
				out.push('[');
				rest = tail;
			},
		}
	}
	out.push_str(rest);
	out
}

fn group(value: i64) -> String {
	let digits = value.unsigned_abs().to_string();
	let mut grouped = String::new();
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
