// Copyright 2021 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use std::{
	env, fs,
	path::{Path, PathBuf},
	process::{self, Command},
};
use toml::value::Table;

/// The name of the project we will building.
const PROJECT_NAME: &str = "validation-worker";
/// The env variable that instructs us to skip the build.
const SKIP_ENV: &str = "SKIP_BUILD";

fn main() {
	if env::var(SKIP_ENV).is_ok() {
		return
	}

	let out_dir = PathBuf::from(env::var("OUT_DIR").expect("`OUT_DIR` is set by cargo"));

	let project = create_project(&out_dir);
	build_project(&project.join("Cargo.toml"));

	fs::copy(project.join("target/release").join(PROJECT_NAME), out_dir.join(PROJECT_NAME))
		.expect("Copies validation worker");
}

fn find_cargo_lock() -> PathBuf {
	let mut path = PathBuf::from(
		env::var("CARGO_MANIFEST_DIR").expect("`CARGO_MANIFEST_DIR` is set by cargo"),
	);

	loop {
		if path.join("Cargo.lock").exists() {
			return path.join("Cargo.lock")
		}

		if !path.pop() {
			panic!("Could not find `Cargo.lock`")
		}
	}
}

fn create_project(out_dir: &Path) -> PathBuf {
	let project_dir = out_dir.join(format!("{}-project", PROJECT_NAME));
	fs::create_dir_all(project_dir.join("src")).expect("Creates project dir and project src dir");

	let mut project_toml = Table::new();

	let mut package = Table::new();
	package.insert("name".into(), PROJECT_NAME.into());
	package.insert("version".into(), "1.0.0".into());
	package.insert("edition".into(), "2021".into());

	project_toml.insert("package".into(), package.into());

	project_toml.insert("workspace".into(), Table::new().into());

	let mut dependencies = Table::new();

	let mut dependency_project = Table::new();
	dependency_project.insert(
		"path".into(),
		env::var("CARGO_MANIFEST_DIR")
			.expect("`CARGO_MANIFEST_DIR` is set by cargo")
			.into(),
	);

	dependencies
		.insert("cumulus-test-relay-validation-worker-provider".into(), dependency_project.into());

	project_toml.insert("dependencies".into(), dependencies.into());

	add_patches(&mut project_toml);

	fs::write(
		project_dir.join("Cargo.toml"),
		toml::to_string_pretty(&project_toml).expect("Wasm workspace toml is valid; qed"),
	)
	.expect("Writes project `Cargo.toml`");

	fs::write(
		project_dir.join("src").join("main.rs"),
		r#"
			cumulus_test_relay_validation_worker_provider::polkadot_node_core_pvf::decl_puppet_worker_main!();
		"#,
	)
	.expect("Writes `main.rs`");

	let cargo_lock = find_cargo_lock();
	fs::copy(&cargo_lock, project_dir.join("Cargo.lock")).expect("Copies `Cargo.lock`");
	println!("cargo:rerun-if-changed={}", cargo_lock.display());

	project_dir
}

fn add_patches(project_toml: &mut Table) {
	let workspace_toml_path = PathBuf::from(
		env::var("CARGO_MANIFEST_DIR").expect("`CARGO_MANIFEST_DIR` is set by cargo"),
	)
	.join("../../Cargo.toml");

	let mut workspace_toml: Table = toml::from_str(
		&fs::read_to_string(&workspace_toml_path).expect("Workspace root `Cargo.toml` exists; qed"),
	)
	.expect("Workspace root `Cargo.toml` is a valid toml file; qed");

	while let Some(mut patch) =
		workspace_toml.remove("patch").and_then(|p| p.try_into::<Table>().ok())
	{
		// Iterate over all patches and make the patch path absolute from the workspace root path.
		patch
			.iter_mut()
			.filter_map(|p| {
				p.1.as_table_mut().map(|t| t.iter_mut().filter_map(|t| t.1.as_table_mut()))
			})
			.flatten()
			.for_each(|p| {
				p.iter_mut().filter(|(k, _)| k == &"path").for_each(|(_, v)| {
					if let Some(path) = v.as_str().map(PathBuf::from) {
						if path.is_relative() {
							*v = workspace_toml_path.join(path).display().to_string().into();
						}
					}
				})
			});

		project_toml.insert("patch".into(), patch.into());
	}
}

fn build_project(cargo_toml: &Path) {
	let cargo = env::var("CARGO").expect("`CARGO` env variable is always set by cargo");

	let status = Command::new(cargo)
		.arg("build")
		.arg("--release")
		.arg(format!("--manifest-path={}", cargo_toml.display()))
		// Unset the `CARGO_TARGET_DIR` to prevent a cargo deadlock (cargo locks a target dir exclusive).
		.env_remove("CARGO_TARGET_DIR")
		// Do not call us recursively.
		.env(SKIP_ENV, "1")
		.status();

	match status.map(|s| s.success()) {
		Ok(true) => {},
		// Use `process.exit(1)` to have a clean error output.
		_ => process::exit(1),
	}
}
