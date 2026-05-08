// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Summarise the test-sim differential suite's open-bug inventory and current
//! regression status.
//!
//! Run from anywhere:
//!
//! ```text
//! cargo run --profile testnet -p polkadot-collator-protocol-test-sim \
//!     --bin divergence-report
//! ```
//!
//! Sections in the output:
//!
//! 1. Suite headline (pass / fail / total, counted from `cargo test`'s
//!    `test result:` line).
//! 2. Real regressions: tests that failed unexpectedly. Empty list = clean.
//! 3. Open bugs: every `bug_on` marker, grouped by tracker URL and impl set.
//!    The marker itself is the source of truth — we walk every `.rs` under
//!    `src/` and parse `#[sim_test(...)]` attributes via `syn`.
//! 4. Stale `should_panic` markers: tests whose impl wrapper passed without
//!    `should_panic` even though the source has `bug_on` for that impl. The
//!    bug got fixed; the marker is now stale and must be removed.
//! 5. Intended divergences: files in `divergent/` that aren't the upcoming-PR
//!    kind — paired tests or impl-only spec deviations.
//!
//! Exits non-zero iff sections 2 or 4 are non-empty (real regression OR stale
//! marker).

use std::{
	collections::BTreeMap,
	path::{Path, PathBuf},
	process::{exit, Command},
};

#[derive(Debug, Clone)]
struct BugMarker {
	/// Test fn (e.g. `core_rotation_accepts_candidates_for_both_cores`).
	fn_name: String,
	/// Module path the test-sim crate uses for this test
	/// (e.g. `scenarios::divergent::upcoming_pr_11967`).
	module_path: String,
	/// Impls the marker covers. Stable order: legacy < experimental.
	impls: Vec<String>,
	/// Tracker URL from `bug_url`, or `None` if absent.
	url: Option<String>,
}

fn main() {
	// `CARGO_MANIFEST_DIR` is this report-tool crate's directory. The test-sim
	// crate lives at `../test-sim`. Workspace root is the first ancestor with
	// `Cargo.lock`.
	let report_manifest = manifest_dir();
	let test_sim_dir = report_manifest
		.parent()
		.expect("report crate has parent")
		.join("test-sim");
	let workspace_root = report_manifest
		.ancestors()
		.find(|p| p.join("Cargo.lock").exists())
		.expect("workspace root with Cargo.lock");

	let test_output = run_cargo_test(workspace_root);
	let (passed, failed, should_panic_ok) = parse_test_result(&test_output);
	let total = passed + failed;
	let failed_tests: Vec<String> = test_output
		.lines()
		.filter_map(|l| {
			let l = l.trim_start_matches("test ");
			let stripped = l.strip_suffix(" ... FAILED")?;
			Some(stripped.to_string())
		})
		.collect();

	let markers = collect_markers(&test_sim_dir.join("src"));
	let stale = stale_markers(&markers, &test_output);

	// --- Section 1 ---
	println!("=== Suite ===");
	println!(
		"  total: {total}   pass: {passed}   fail: {failed}   \
		(of pass, {should_panic_ok} are should_panic / known broken)"
	);

	// --- Section 2 ---
	println!();
	println!("=== Real regressions ===");
	if failed_tests.is_empty() {
		println!("  none");
	} else {
		for t in &failed_tests {
			println!("  {t}");
		}
	}

	// --- Section 3 ---
	println!();
	println!("=== Open bugs (grouped by tracker) ===");
	if markers.is_empty() {
		println!("  none");
	} else {
		// Group by (url, impls).
		let mut groups: BTreeMap<(String, String), usize> = BTreeMap::new();
		for m in &markers {
			let url = m.url.clone().unwrap_or_else(|| "(no url)".to_string());
			let impls = m.impls.join(",");
			*groups.entry((url, impls)).or_default() += 1;
		}
		// Sort by count DESC, then url, then impls.
		let mut entries: Vec<_> = groups.into_iter().collect();
		entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
		for ((url, impls), count) in entries {
			println!("  {count:3}  [{impls:<25}]  {url}");
			// If the tracker is a `memory:` reference, surface the memory file's
			// frontmatter `description:` as a one-line root-cause summary so the
			// reader doesn't have to chase the file to know what's broken.
			if let Some(rel) = url.strip_prefix("memory:") {
				if let Some(summary) = read_memory_summary(rel) {
					println!("       └─ {summary}");
				}
			}
		}
	}

	// --- Section 4 ---
	println!();
	println!("=== Stale should_panic markers (bug fixed; remove the marker) ===");
	if stale.is_empty() {
		println!("  none");
	} else {
		for entry in &stale {
			println!("  {entry}");
		}
	}

	// --- Section 5 ---
	println!();
	println!("=== Intended divergences ===");
	let divergent_dir = test_sim_dir.join("src/scenarios/divergent");
	if divergent_dir.is_dir() {
		let mut names: Vec<String> = std::fs::read_dir(&divergent_dir)
			.expect("read divergent dir")
			.filter_map(|e| e.ok())
			.filter_map(|e| {
				let p = e.path();
				if p.extension()?.to_str()? != "rs" {
					return None;
				}
				let stem = p.file_stem()?.to_str()?.to_string();
				if stem == "mod" || stem.starts_with("upcoming_pr_") {
					return None;
				}
				Some(stem)
			})
			.collect();
		names.sort();
		for n in names {
			println!("  {n}");
		}
	}

	if !failed_tests.is_empty() || !stale.is_empty() {
		exit(1);
	}
}

/// Path to this crate's `Cargo.toml` directory.
fn manifest_dir() -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// For a `memory:<name>` tracker URL, read the matching memory file's frontmatter
/// `description:` and return the one-liner root-cause summary. Returns `None` if
/// the memory dir or file isn't found, or if the frontmatter has no description.
///
/// Looks for the file at `~/.claude/projects/<project>/memory/<name>.md` for any
/// project subdir whose name suggests a polkadot-sdk worktree. Falls back to
/// scanning the user's `.claude/projects` if the conventional path misses.
fn read_memory_summary(name: &str) -> Option<String> {
	let home = std::env::var_os("HOME")?;
	let memory_root = PathBuf::from(home).join(".claude/projects");
	if !memory_root.is_dir() {
		return None;
	}
	// Prefer a project dir whose name encodes "polkadot-sdk".
	let projects: Vec<PathBuf> = std::fs::read_dir(&memory_root)
		.ok()?
		.filter_map(|e| e.ok())
		.map(|e| e.path())
		.filter(|p| p.is_dir())
		.collect();
	let candidates: Vec<PathBuf> = projects
		.iter()
		.filter(|p| {
			p.file_name()
				.and_then(|n| n.to_str())
				.is_some_and(|n| n.contains("polkadot-sdk"))
		})
		.cloned()
		.collect();
	let search_set = if candidates.is_empty() { projects } else { candidates };
	for project in search_set {
		let path = project.join("memory").join(format!("{name}.md"));
		if let Ok(text) = std::fs::read_to_string(&path) {
			return parse_frontmatter_description(&text);
		}
	}
	None
}

fn parse_frontmatter_description(text: &str) -> Option<String> {
	// Frontmatter is delimited by `---` on its own lines. Look for `description:`
	// inside.
	let mut lines = text.lines();
	if lines.next()? != "---" {
		return None;
	}
	for line in lines {
		if line == "---" {
			break;
		}
		if let Some(rest) = line.strip_prefix("description:") {
			return Some(rest.trim().to_string());
		}
	}
	None
}

fn run_cargo_test(workspace_root: &Path) -> String {
	eprintln!("running cargo test (this may take a moment)...");
	let out = Command::new("cargo")
		.args([
			"test",
			"--profile",
			"testnet",
			"-p",
			"polkadot-collator-protocol-test-sim",
		])
		.current_dir(workspace_root)
		.output()
		.expect("spawn cargo test");
	// Tests can fail without the cargo invocation itself failing; either way we
	// want the captured output for parsing.
	let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
	s.push_str(&String::from_utf8_lossy(&out.stderr));
	s
}

fn parse_test_result(output: &str) -> (usize, usize, usize) {
	// Look for the lib's `test result: ok. N passed; M failed; ...` line. Use
	// the first one (lib tests; doctests are second and irrelevant here).
	let mut passed = 0;
	let mut failed = 0;
	for line in output.lines() {
		let trimmed = line.trim_start();
		if let Some(rest) = trimmed.strip_prefix("test result:") {
			passed = extract_count(rest, "passed");
			failed = extract_count(rest, "failed");
			break;
		}
	}
	let should_panic_ok = output
		.lines()
		.filter(|l| l.contains("should panic") && l.ends_with("ok"))
		.count();
	(passed, failed, should_panic_ok)
}

fn extract_count(text: &str, label: &str) -> usize {
	for token in text.split([';', '.', ' ']) {
		let token = token.trim();
		if let Some(num) = token.strip_suffix(label) {
			if let Ok(n) = num.trim().parse::<usize>() {
				return n;
			}
		}
	}
	// Format is `... N passed; M failed ...`, so try the simpler split-once.
	if let Some(idx) = text.find(label) {
		let prefix = text[..idx].trim_end();
		if let Some(num) = prefix.split_whitespace().next_back() {
			if let Ok(n) = num.parse::<usize>() {
				return n;
			}
		}
	}
	0
}

/// Walk `src/` and parse every `#[sim_test(...)]` (or `#[crate::sim_test(...)]`)
/// attribute that carries a `bug_on` flag. Returns one `BugMarker` per impl per
/// attribute (so a test with `bug_on = "legacy"` AND `bug_on = "experimental"`
/// produces ONE marker with `impls = ["legacy", "experimental"]`).
fn collect_markers(src: &Path) -> Vec<BugMarker> {
	let mut out = Vec::new();
	for entry in walkdir::WalkDir::new(src) {
		let entry = match entry {
			Ok(e) => e,
			Err(_) => continue,
		};
		if entry.file_type().is_file() && entry.path().extension().is_some_and(|e| e == "rs") {
			let path = entry.path();
			let text = match std::fs::read_to_string(path) {
				Ok(t) => t,
				Err(_) => continue,
			};
			let file = match syn::parse_file(&text) {
				Ok(f) => f,
				Err(_) => continue,
			};
			let module_path = source_path_to_module(src, path);
			extract_markers_from_file(&file, &module_path, &mut out);
		}
	}
	out
}

fn source_path_to_module(src_root: &Path, path: &Path) -> String {
	let rel = path.strip_prefix(src_root).unwrap_or(path);
	let mut comps: Vec<String> = rel
		.with_extension("")
		.components()
		.map(|c| c.as_os_str().to_string_lossy().into_owned())
		.collect();
	if comps.last().map(|s| s.as_str()) == Some("mod") {
		comps.pop();
	}
	if comps.last().map(|s| s.as_str()) == Some("lib") {
		comps.pop();
	}
	comps.join("::")
}

fn extract_markers_from_file(
	file: &syn::File,
	module_path: &str,
	out: &mut Vec<BugMarker>,
) {
	for item in &file.items {
		visit_item(item, module_path, out);
	}
}

fn visit_item(item: &syn::Item, module_path: &str, out: &mut Vec<BugMarker>) {
	match item {
		syn::Item::Fn(item_fn) => {
			if let Some(marker) = parse_sim_test_attrs(&item_fn.attrs) {
				out.push(BugMarker {
					fn_name: item_fn.sig.ident.to_string(),
					module_path: module_path.to_string(),
					impls: marker.impls,
					url: marker.url,
				});
			}
		},
		syn::Item::Mod(item_mod) => {
			if let Some((_, items)) = &item_mod.content {
				let inner = format!("{module_path}::{}", item_mod.ident);
				for it in items {
					visit_item(it, &inner, out);
				}
			}
		},
		_ => {},
	}
}

#[derive(Default)]
struct ParsedAttr {
	impls: Vec<String>,
	url: Option<String>,
}

fn parse_sim_test_attrs(attrs: &[syn::Attribute]) -> Option<ParsedAttr> {
	let mut acc = ParsedAttr::default();
	let mut found_sim_test = false;
	for attr in attrs {
		if !is_sim_test_path(attr.path()) {
			continue;
		}
		found_sim_test = true;
		// sim_test takes a comma-separated list of `key = "value"` pairs.
		let _ = attr.parse_nested_meta(|meta| {
			let key = meta
				.path
				.get_ident()
				.map(|i| i.to_string())
				.unwrap_or_default();
			let value: syn::LitStr = meta.value()?.parse()?;
			let value = value.value();
			match key.as_str() {
				"bug_on" => {
					if !acc.impls.iter().any(|i| i == &value) {
						acc.impls.push(value);
					}
				},
				"bug_url" => acc.url = Some(value),
				_ => {}, // only/skip and the like — not relevant for the report
			}
			Ok(())
		});
	}
	if !found_sim_test || acc.impls.is_empty() {
		return None;
	}
	// Stable order: legacy first, then experimental.
	acc.impls.sort();
	Some(acc)
}

fn is_sim_test_path(path: &syn::Path) -> bool {
	// Match either `sim_test`, `crate::sim_test`, or any path whose last
	// segment is `sim_test`.
	path.segments.last().is_some_and(|s| s.ident == "sim_test")
}

/// A stale marker = a `bug_on` impl that passed *without* `should_panic`. When
/// cargo prints `test foo - should panic ... ok` the marker is load-bearing.
/// Plain `test foo ... ok` for a marked impl means the bug got fixed.
fn stale_markers(markers: &[BugMarker], test_output: &str) -> Vec<String> {
	let mut stale = Vec::new();
	for marker in markers {
		for impl_name in &marker.impls {
			let test_id = format!("{}::{}__{}", marker.module_path, marker.fn_name, impl_name);
			let ok_line = format!("test {test_id} ... ok");
			let panic_line_prefix = format!("test {test_id} - should panic");
			let plain_ok = test_output.lines().any(|l| l.trim() == ok_line);
			let panic_ok = test_output
				.lines()
				.any(|l| l.starts_with(&panic_line_prefix) && l.ends_with("ok"));
			if plain_ok && !panic_ok {
				stale.push(test_id);
			}
		}
	}
	stale.sort();
	stale.dedup();
	stale
}
