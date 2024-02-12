// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::{write_file_if_changed, CargoCommandVersioned, RuntimeTarget, OFFLINE};

use build_helper::rerun_if_changed;
use cargo_metadata::{DependencyKind, Metadata, MetadataCommand};
use console::style;
use parity_wasm::elements::{deserialize_buffer, Module};
use std::{
	borrow::ToOwned,
	collections::HashSet,
	env, fs,
	hash::{Hash, Hasher},
	ops::Deref,
	path::{Path, PathBuf},
	process,
};
use strum::{EnumIter, IntoEnumIterator};
use toml::value::Table;
use walkdir::WalkDir;

/// Colorize an info message.
///
/// Returns the colorized message.
fn colorize_info_message(message: &str) -> String {
	if super::color_output_enabled() {
		style(message).yellow().bold().to_string()
	} else {
		message.into()
	}
}

/// Holds the path to the bloaty WASM binary.
pub struct WasmBinaryBloaty(PathBuf);

impl WasmBinaryBloaty {
	/// Returns the escaped path to the bloaty binary.
	pub fn bloaty_path_escaped(&self) -> String {
		self.0.display().to_string().escape_default().to_string()
	}

	/// Returns the path to the binary.
	pub fn bloaty_path(&self) -> &Path {
		&self.0
	}
}

/// Holds the path to the WASM binary.
pub struct WasmBinary(PathBuf);

impl WasmBinary {
	/// Returns the path to the wasm binary.
	pub fn wasm_binary_path(&self) -> &Path {
		&self.0
	}

	/// Returns the escaped path to the wasm binary.
	pub fn wasm_binary_path_escaped(&self) -> String {
		self.0.display().to_string().escape_default().to_string()
	}
}

fn crate_metadata(cargo_manifest: &Path) -> Metadata {
	let mut cargo_lock = cargo_manifest.to_path_buf();
	cargo_lock.set_file_name("Cargo.lock");

	let cargo_lock_existed = cargo_lock.exists();

	// If we can find a `Cargo.lock`, we assume that this is the workspace root and there exists a
	// `Cargo.toml` that we can use for getting the metadata.
	let cargo_manifest = if let Some(mut cargo_lock) = find_cargo_lock(cargo_manifest) {
		cargo_lock.set_file_name("Cargo.toml");
		cargo_lock
	} else {
		cargo_manifest.to_path_buf()
	};

	let crate_metadata_command = create_metadata_command(cargo_manifest);

	let crate_metadata = crate_metadata_command
		.exec()
		.expect("`cargo metadata` can not fail on project `Cargo.toml`; qed");
	// If the `Cargo.lock` didn't exist, we need to remove it after
	// calling `cargo metadata`. This is required to ensure that we don't change
	// the build directory outside of the `target` folder. Commands like
	// `cargo publish` require this.
	if !cargo_lock_existed {
		let _ = fs::remove_file(&cargo_lock);
	}

	crate_metadata
}

/// Creates the WASM project, compiles the WASM binary and compacts the WASM binary.
///
/// # Returns
///
/// The path to the compact runtime binary and the bloaty runtime binary.
pub(crate) fn create_and_compile(
	target: RuntimeTarget,
	project_cargo_toml: &Path,
	default_rustflags: &str,
	cargo_cmd: CargoCommandVersioned,
	features_to_enable: Vec<String>,
	bloaty_blob_out_name_override: Option<String>,
	check_for_runtime_version_section: bool,
) -> (Option<WasmBinary>, WasmBinaryBloaty) {
	let runtime_workspace_root = get_wasm_workspace_root();
	let runtime_workspace = runtime_workspace_root.join(target.build_subdirectory());

	let crate_metadata = crate_metadata(project_cargo_toml);

	let project = create_project(
		target,
		project_cargo_toml,
		&runtime_workspace,
		&crate_metadata,
		crate_metadata.workspace_root.as_ref(),
		features_to_enable,
	);

	let build_config = BuildConfiguration::detect(target, &project);

	// Build the bloaty runtime blob
	let raw_blob_path = build_bloaty_blob(
		target,
		&build_config.blob_build_profile,
		&project,
		default_rustflags,
		cargo_cmd,
	);

	let (final_blob_binary, bloaty_blob_binary) = match target {
		RuntimeTarget::Wasm => compile_wasm(
			project_cargo_toml,
			&project,
			bloaty_blob_out_name_override,
			check_for_runtime_version_section,
			&build_config,
		),
		RuntimeTarget::Riscv => {
			let out_name = bloaty_blob_out_name_override
				.unwrap_or_else(|| get_blob_name(target, project_cargo_toml));
			let out_path = project.join(format!("{out_name}.polkavm"));
			fs::copy(raw_blob_path, &out_path).expect("copying the runtime blob should never fail");
			(None, WasmBinaryBloaty(out_path))
		},
	};

	generate_rerun_if_changed_instructions(
		project_cargo_toml,
		&project,
		&runtime_workspace,
		final_blob_binary.as_ref(),
		&bloaty_blob_binary,
	);

	if let Err(err) = adjust_mtime(&bloaty_blob_binary, final_blob_binary.as_ref()) {
		build_helper::warning!("Error while adjusting the mtime of the blob binaries: {}", err)
	}

	(final_blob_binary, bloaty_blob_binary)
}

fn compile_wasm(
	project_cargo_toml: &Path,
	project: &Path,
	bloaty_blob_out_name_override: Option<String>,
	check_for_runtime_version_section: bool,
	build_config: &BuildConfiguration,
) -> (Option<WasmBinary>, WasmBinaryBloaty) {
	// Get the name of the bloaty runtime blob.
	let bloaty_blob_default_name = get_blob_name(RuntimeTarget::Wasm, project_cargo_toml);
	let bloaty_blob_out_name =
		bloaty_blob_out_name_override.unwrap_or_else(|| bloaty_blob_default_name.clone());

	let bloaty_blob_binary = copy_bloaty_blob(
		&project,
		&build_config.blob_build_profile,
		&bloaty_blob_default_name,
		&bloaty_blob_out_name,
	);

	// Try to compact and compress the bloaty blob, if the *outer* profile wants it.
	//
	// This is because, by default the inner profile will be set to `Release` even when the outer
	// profile is `Debug`, because the blob built in `Debug` profile is too slow for normal
	// development activities.
	let (compact_blob_path, compact_compressed_blob_path) =
		if build_config.outer_build_profile.wants_compact() {
			let compact_blob_path = compact_wasm(
				&project,
				&build_config.blob_build_profile,
				project_cargo_toml,
				&bloaty_blob_out_name,
			);
			let compact_compressed_blob_path = compact_blob_path
				.as_ref()
				.and_then(|p| try_compress_blob(&p.0, &bloaty_blob_out_name));
			(compact_blob_path, compact_compressed_blob_path)
		} else {
			(None, None)
		};

	if check_for_runtime_version_section {
		ensure_runtime_version_wasm_section_exists(bloaty_blob_binary.bloaty_path());
	}

	compact_blob_path
		.as_ref()
		.map(|wasm_binary| copy_blob_to_target_directory(project_cargo_toml, wasm_binary));

	compact_compressed_blob_path.as_ref().map(|wasm_binary_compressed| {
		copy_blob_to_target_directory(project_cargo_toml, wasm_binary_compressed)
	});

	let final_blob_binary = compact_compressed_blob_path.or(compact_blob_path);
	(final_blob_binary, bloaty_blob_binary)
}

/// Ensures that the `runtime_version` section exists in the given blob.
///
/// If the section can not be found, it will print an error and exit the builder.
fn ensure_runtime_version_wasm_section_exists(blob_path: &Path) {
	let blob = fs::read(blob_path).expect("`{blob_path}` was just written and should exist; qed");

	let module: Module = match deserialize_buffer(&blob) {
		Ok(m) => m,
		Err(e) => {
			println!("Failed to deserialize `{}`: {e:?}", blob_path.display());
			process::exit(1);
		},
	};

	if !module.custom_sections().any(|cs| cs.name() == "runtime_version") {
		println!(
			"Couldn't find the `runtime_version` section. \
				  Please ensure that you are using the `sp_version::runtime_version` attribute macro!"
		);
		process::exit(1);
	}
}

/// Adjust the mtime of the bloaty and compressed/compact wasm files.
///
/// We add the bloaty and the compressed/compact wasm file to the `rerun-if-changed` files.
/// Cargo/Rustc determines based on the timestamp of the `invoked.timestamp` file that can be found
/// in the `OUT_DIR/..`, if it needs to rerun a `build.rs` script. The problem is that this
/// `invoked.timestamp` is created when the `build.rs` is executed and the wasm binaries are created
/// later. This leads to them having a later mtime than the `invoked.timestamp` file and thus,
/// cargo/rustc always re-executes the `build.rs` script. To hack around this, we copy the mtime of
/// the `invoked.timestamp` to the wasm binaries.
fn adjust_mtime(
	bloaty_wasm: &WasmBinaryBloaty,
	compressed_or_compact_wasm: Option<&WasmBinary>,
) -> std::io::Result<()> {
	let out_dir = build_helper::out_dir();
	let invoked_timestamp = out_dir.join("../invoked.timestamp");

	// Get the mtime of the `invoked.timestamp`
	let metadata = fs::metadata(invoked_timestamp)?;
	let mtime = filetime::FileTime::from_last_modification_time(&metadata);

	filetime::set_file_mtime(bloaty_wasm.bloaty_path(), mtime)?;
	if let Some(binary) = compressed_or_compact_wasm.as_ref() {
		filetime::set_file_mtime(binary.wasm_binary_path(), mtime)?;
	}

	Ok(())
}

/// Find the `Cargo.lock` relative to the `OUT_DIR` environment variable.
///
/// If the `Cargo.lock` cannot be found, we emit a warning and return `None`.
fn find_cargo_lock(cargo_manifest: &Path) -> Option<PathBuf> {
	fn find_impl(mut path: PathBuf) -> Option<PathBuf> {
		loop {
			if path.join("Cargo.lock").exists() {
				return Some(path.join("Cargo.lock"))
			}

			if !path.pop() {
				return None
			}
		}
	}

	if let Ok(workspace) = env::var(crate::WASM_BUILD_WORKSPACE_HINT) {
		let path = PathBuf::from(workspace);

		if path.join("Cargo.lock").exists() {
			return Some(path.join("Cargo.lock"))
		} else {
			build_helper::warning!(
				"`{}` env variable doesn't point to a directory that contains a `Cargo.lock`.",
				crate::WASM_BUILD_WORKSPACE_HINT,
			);
		}
	}

	if let Some(path) = find_impl(build_helper::out_dir()) {
		return Some(path)
	}

	build_helper::warning!(
		"Could not find `Cargo.lock` for `{}`, while searching from `{}`. \
		 To fix this, point the `{}` env variable to the directory of the workspace being compiled.",
		cargo_manifest.display(),
		build_helper::out_dir().display(),
		crate::WASM_BUILD_WORKSPACE_HINT,
	);

	None
}

/// Extract the crate name from the given `Cargo.toml`.
fn get_crate_name(cargo_manifest: &Path) -> String {
	let cargo_toml: Table = toml::from_str(
		&fs::read_to_string(cargo_manifest).expect("File exists as checked before; qed"),
	)
	.expect("Cargo manifest is a valid toml file; qed");

	let package = cargo_toml
		.get("package")
		.and_then(|t| t.as_table())
		.expect("`package` key exists in valid `Cargo.toml`; qed");

	package
		.get("name")
		.and_then(|p| p.as_str())
		.map(ToOwned::to_owned)
		.expect("Package name exists; qed")
}

/// Returns the name for the blob binary.
fn get_blob_name(target: RuntimeTarget, cargo_manifest: &Path) -> String {
	let crate_name = get_crate_name(cargo_manifest);
	match target {
		RuntimeTarget::Wasm => crate_name.replace('-', "_"),
		RuntimeTarget::Riscv => crate_name,
	}
}

/// Returns the root path of the wasm workspace.
fn get_wasm_workspace_root() -> PathBuf {
	let mut out_dir = build_helper::out_dir();

	loop {
		match out_dir.parent() {
			Some(parent) if out_dir.ends_with("build") => return parent.to_path_buf(),
			_ =>
				if !out_dir.pop() {
					break
				},
		}
	}

	panic!("Could not find target dir in: {}", build_helper::out_dir().display())
}

fn create_project_cargo_toml(
	target: RuntimeTarget,
	wasm_workspace: &Path,
	workspace_root_path: &Path,
	crate_name: &str,
	crate_path: &Path,
	wasm_binary: &str,
	enabled_features: impl Iterator<Item = String>,
) {
	let mut workspace_toml: Table = toml::from_str(
		&fs::read_to_string(workspace_root_path.join("Cargo.toml"))
			.expect("Workspace root `Cargo.toml` exists; qed"),
	)
	.expect("Workspace root `Cargo.toml` is a valid toml file; qed");

	let mut wasm_workspace_toml = Table::new();

	// Add different profiles which are selected by setting `WASM_BUILD_TYPE`.
	let mut release_profile = Table::new();
	release_profile.insert("panic".into(), "abort".into());
	release_profile.insert("lto".into(), "thin".into());

	let mut production_profile = Table::new();
	production_profile.insert("inherits".into(), "release".into());
	production_profile.insert("lto".into(), "fat".into());
	production_profile.insert("codegen-units".into(), 1.into());

	let mut dev_profile = Table::new();
	dev_profile.insert("panic".into(), "abort".into());

	let mut profile = Table::new();
	profile.insert("release".into(), release_profile.into());
	profile.insert("production".into(), production_profile.into());
	profile.insert("dev".into(), dev_profile.into());

	wasm_workspace_toml.insert("profile".into(), profile.into());

	// Add patch section from the project root `Cargo.toml`
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
							*v = workspace_root_path.join(path).display().to_string().into();
						}
					}
				})
			});

		wasm_workspace_toml.insert("patch".into(), patch.into());
	}

	let mut package = Table::new();
	package.insert("name".into(), format!("{}-blob", crate_name).into());
	package.insert("version".into(), "1.0.0".into());
	package.insert("edition".into(), "2021".into());

	wasm_workspace_toml.insert("package".into(), package.into());

	if target == RuntimeTarget::Wasm {
		let mut lib = Table::new();
		lib.insert("name".into(), wasm_binary.into());
		lib.insert("crate-type".into(), vec!["cdylib".to_string()].into());
		wasm_workspace_toml.insert("lib".into(), lib.into());
	}

	let mut dependencies = Table::new();

	let mut wasm_project = Table::new();
	wasm_project.insert("package".into(), crate_name.into());
	wasm_project.insert("path".into(), crate_path.display().to_string().into());
	wasm_project.insert("default-features".into(), false.into());
	wasm_project.insert("features".into(), enabled_features.collect::<Vec<_>>().into());

	dependencies.insert("wasm-project".into(), wasm_project.into());

	wasm_workspace_toml.insert("dependencies".into(), dependencies.into());

	wasm_workspace_toml.insert("workspace".into(), Table::new().into());

	if target == RuntimeTarget::Riscv {
		// This dependency currently doesn't compile under RISC-V, so patch it with our own fork.
		//
		// TODO: Remove this once a new version of `bitvec` (which uses a new version of `radium`
		//       which doesn't have this problem) is released on crates.io.
		let patch = toml::toml! {
			[crates-io]
			radium = { git = "https://github.com/paritytech/radium-0.7-fork.git", rev = "a5da15a15c90fd169d661d206cf0db592487f52b" }
		};
		wasm_workspace_toml.insert("patch".into(), patch.into());
	}

	write_file_if_changed(
		wasm_workspace.join("Cargo.toml"),
		toml::to_string_pretty(&wasm_workspace_toml).expect("Wasm workspace toml is valid; qed"),
	);
}

/// Find a package by the given `manifest_path` in the metadata. In case it can't be found by its
/// manifest_path, fallback to finding it by name; this is necessary during publish because the
/// package's manifest path will be *generated* within a specific packaging directory, thus it won't
/// be found by its original path anymore.
///
/// Panics if the package could not be found.
fn find_package_by_manifest_path<'a>(
	pkg_name: &str,
	manifest_path: &Path,
	crate_metadata: &'a cargo_metadata::Metadata,
) -> &'a cargo_metadata::Package {
	if let Some(pkg) = crate_metadata.packages.iter().find(|p| p.manifest_path == manifest_path) {
		return pkg
	}

	let pkgs_by_name = crate_metadata
		.packages
		.iter()
		.filter(|p| p.name == pkg_name)
		.collect::<Vec<_>>();

	if let Some(pkg) = pkgs_by_name.first() {
		if pkgs_by_name.len() > 1 {
			panic!(
				"Found multiple packages matching the name {pkg_name} ({manifest_path:?}): {:?}",
				pkgs_by_name
			);
		} else {
			return pkg
		}
	} else {
		panic!("Failed to find entry for package {pkg_name} ({manifest_path:?}).");
	}
}

/// Get a list of enabled features for the project.
fn project_enabled_features(
	pkg_name: &str,
	cargo_manifest: &Path,
	crate_metadata: &cargo_metadata::Metadata,
) -> Vec<String> {
	let package = find_package_by_manifest_path(pkg_name, cargo_manifest, crate_metadata);

	let std_enabled = package.features.get("std");

	let mut enabled_features = package
		.features
		.iter()
		.filter(|(f, v)| {
			let mut feature_env = f.replace("-", "_");
			feature_env.make_ascii_uppercase();

			// If this is a feature that corresponds only to an optional dependency
			// and this feature is enabled by the `std` feature, we assume that this
			// is only done through the `std` feature. This is a bad heuristic and should
			// be removed after namespaced features are landed:
			// https://doc.rust-lang.org/cargo/reference/unstable.html#namespaced-features
			// Then we can just express this directly in the `Cargo.toml` and do not require
			// this heuristic anymore. However, for the transition phase between now and namespaced
			// features already being present in nightly, we need this code to make
			// runtimes compile with all the possible rustc versions.
			if v.len() == 1 &&
				v.get(0).map_or(false, |v| *v == format!("dep:{}", f)) &&
				std_enabled.as_ref().map(|e| e.iter().any(|ef| ef == *f)).unwrap_or(false)
			{
				return false
			}

			// We don't want to enable the `std`/`default` feature for the wasm build and
			// we need to check if the feature is enabled by checking the env variable.
			*f != "std" &&
				*f != "default" && env::var(format!("CARGO_FEATURE_{}", feature_env))
				.map(|v| v == "1")
				.unwrap_or_default()
		})
		.map(|d| d.0.clone())
		.collect::<Vec<_>>();

	enabled_features.sort();
	enabled_features
}

/// Returns if the project has the `runtime-wasm` feature
fn has_runtime_wasm_feature_declared(
	pkg_name: &str,
	cargo_manifest: &Path,
	crate_metadata: &cargo_metadata::Metadata,
) -> bool {
	let package = find_package_by_manifest_path(pkg_name, cargo_manifest, crate_metadata);

	package.features.keys().any(|k| k == "runtime-wasm")
}

/// Create the project used to build the wasm binary.
///
/// # Returns
///
/// The path to the created wasm project.
fn create_project(
	target: RuntimeTarget,
	project_cargo_toml: &Path,
	wasm_workspace: &Path,
	crate_metadata: &Metadata,
	workspace_root_path: &Path,
	features_to_enable: Vec<String>,
) -> PathBuf {
	let crate_name = get_crate_name(project_cargo_toml);
	let crate_path = project_cargo_toml.parent().expect("Parent path exists; qed");
	let wasm_binary = get_blob_name(target, project_cargo_toml);
	let wasm_project_folder = wasm_workspace.join(&crate_name);

	fs::create_dir_all(wasm_project_folder.join("src"))
		.expect("Wasm project dir create can not fail; qed");

	let mut enabled_features =
		project_enabled_features(&crate_name, project_cargo_toml, crate_metadata);

	if has_runtime_wasm_feature_declared(&crate_name, project_cargo_toml, crate_metadata) {
		enabled_features.push("runtime-wasm".into());
	}

	let mut enabled_features = enabled_features.into_iter().collect::<HashSet<_>>();
	enabled_features.extend(features_to_enable.into_iter());

	create_project_cargo_toml(
		target,
		&wasm_project_folder,
		workspace_root_path,
		&crate_name,
		crate_path,
		&wasm_binary,
		enabled_features.into_iter(),
	);

	match target {
		RuntimeTarget::Wasm => {
			write_file_if_changed(
				wasm_project_folder.join("src/lib.rs"),
				"#![no_std] pub use wasm_project::*;",
			);
		},
		RuntimeTarget::Riscv => {
			write_file_if_changed(
				wasm_project_folder.join("src/main.rs"),
				"#![no_std] #![no_main] pub use wasm_project::*;",
			);
		},
	}

	if let Some(crate_lock_file) = find_cargo_lock(project_cargo_toml) {
		// Use the `Cargo.lock` of the main project.
		crate::copy_file_if_changed(crate_lock_file, wasm_project_folder.join("Cargo.lock"));
	}

	wasm_project_folder
}

/// A rustc profile.
#[derive(Clone, Debug, EnumIter)]
enum Profile {
	/// The `--profile dev` profile.
	Debug,
	/// The `--profile release` profile.
	Release,
	/// The `--profile production` profile.
	Production,
}

impl Profile {
	/// The name of the profile as supplied to the cargo `--profile` cli option.
	fn name(&self) -> &'static str {
		match self {
			Self::Debug => "dev",
			Self::Release => "release",
			Self::Production => "production",
		}
	}

	/// The sub directory within `target` where cargo places the build output.
	///
	/// # Note
	///
	/// Usually this is the same as [`Self::name`] with the exception of the debug
	/// profile which is called `dev`.
	fn directory(&self) -> &'static str {
		match self {
			Self::Debug => "debug",
			_ => self.name(),
		}
	}

	/// Whether the resulting binary should be compacted and compressed.
	fn wants_compact(&self) -> bool {
		!matches!(self, Self::Debug)
	}
}

/// The build configuration for this build.
#[derive(Debug)]
struct BuildConfiguration {
	/// The profile that is used to build the outer project.
	pub outer_build_profile: Profile,
	/// The profile to use to build the runtime blob.
	pub blob_build_profile: Profile,
}

impl BuildConfiguration {
	/// Create a [`BuildConfiguration`] by detecting which profile is used for the main build and
	/// checking any env var overrides.
	///
	/// We cannot easily determine the profile that is used by the main cargo invocation
	/// because the `PROFILE` environment variable won't contain any custom profiles like
	/// "production". It would only contain the builtin profile where the custom profile
	/// inherits from. This is why we inspect the build path to learn which profile is used.
	///
	/// When not overriden by a env variable we always default to building wasm with the `Release`
	/// profile even when the main build uses the debug build. This is because wasm built with the
	/// `Debug` profile is too slow for normal development activities and almost never intended.
	///
	/// When cargo is building in `--profile dev`, user likely intends to compile fast, so we don't
	/// bother producing compact or compressed blobs.
	///
	/// # Note
	///
	/// Can be overriden by setting [`crate::WASM_BUILD_TYPE_ENV`].
	fn detect(target: RuntimeTarget, wasm_project: &Path) -> Self {
		let (name, overriden) = if let Ok(name) = env::var(crate::WASM_BUILD_TYPE_ENV) {
			(name, true)
		} else {
			// First go backwards to the beginning of the target directory.
			// Then go forwards to find the build subdirectory.
			// We need to go backwards first because when starting from the root there
			// might be a chance that someone has a directory somewhere in the path with the same
			// name.
			let name = wasm_project
				.components()
				.rev()
				.take_while(|c| c.as_os_str() != "target")
				.collect::<Vec<_>>()
				.iter()
				.rev()
				.take_while(|c| c.as_os_str() != target.build_subdirectory())
				.last()
				.expect("We put the runtime project within a `target/.../[rw]build` path; qed")
				.as_os_str()
				.to_str()
				.expect("All our profile directory names are ascii; qed")
				.to_string();
			(name, false)
		};
		let outer_build_profile = Profile::iter().find(|p| p.directory() == name);
		let blob_build_profile = match (outer_build_profile.clone(), overriden) {
			// When not overriden by a env variable we default to using the `Release` profile
			// for the wasm build even when the main build uses the debug build. This
			// is because the `Debug` profile is too slow for normal development activities.
			(Some(Profile::Debug), false) => Profile::Release,
			// For any other profile or when overriden we take it at face value.
			(Some(profile), _) => profile,
			// For non overriden unknown profiles we fall back to `Release`.
			// This allows us to continue building when a custom profile is used for the
			// main builds cargo. When explicitly passing a profile via env variable we are
			// not doing a fallback.
			(None, false) => {
				let profile = Profile::Release;
				build_helper::warning!(
					"Unknown cargo profile `{}`. Defaulted to `{:?}` for the runtime build.",
					name,
					profile,
				);
				profile
			},
			// Invalid profile specified.
			(None, true) => {
				// We use println! + exit instead of a panic in order to have a cleaner output.
				println!(
					"Unexpected profile name: `{}`. One of the following is expected: {:?}",
					name,
					Profile::iter().map(|p| p.directory()).collect::<Vec<_>>(),
				);
				process::exit(1);
			},
		};
		BuildConfiguration {
			outer_build_profile: outer_build_profile.unwrap_or(Profile::Release),
			blob_build_profile,
		}
	}
}

/// Check environment whether we should build without network
fn offline_build() -> bool {
	env::var(OFFLINE).map_or(false, |v| v == "true")
}

/// Build the project and create the bloaty runtime blob.
fn build_bloaty_blob(
	target: RuntimeTarget,
	blob_build_profile: &Profile,
	project: &Path,
	default_rustflags: &str,
	cargo_cmd: CargoCommandVersioned,
) -> PathBuf {
	let manifest_path = project.join("Cargo.toml");
	let mut build_cmd = cargo_cmd.command();

	let mut rustflags = String::new();
	match target {
		RuntimeTarget::Wasm => {
			rustflags.push_str(
				"-C target-cpu=mvp -C target-feature=-sign-ext -C link-arg=--export-table ",
			);
		},
		RuntimeTarget::Riscv => {
			rustflags.push_str("-C target-feature=+lui-addi-fusion -C relocation-model=pie -C link-arg=--emit-relocs -C link-arg=--unique ");
		},
	}

	rustflags.push_str(default_rustflags);
	rustflags.push_str(" --cfg substrate_runtime ");
	rustflags.push_str(&env::var(crate::WASM_BUILD_RUSTFLAGS_ENV).unwrap_or_default());

	build_cmd
		.arg("rustc")
		.arg(format!("--target={}", target.rustc_target()))
		.arg(format!("--manifest-path={}", manifest_path.display()))
		.env("RUSTFLAGS", rustflags)
		// Manually set the `CARGO_TARGET_DIR` to prevent a cargo deadlock (cargo locks a target dir
		// exclusive). The runner project is created in `CARGO_TARGET_DIR` and executing it will
		// create a sub target directory inside of `CARGO_TARGET_DIR`.
		.env("CARGO_TARGET_DIR", &project.join("target").display().to_string())
		// As we are being called inside a build-script, this env variable is set. However, we set
		// our own `RUSTFLAGS` and thus, we need to remove this. Otherwise cargo favors this
		// env variable.
		.env_remove("CARGO_ENCODED_RUSTFLAGS")
		// Make sure if we're called from within a `build.rs` the host toolchain won't override a
		// rustup toolchain we've picked.
		.env_remove("RUSTC")
		// We don't want to call ourselves recursively
		.env(crate::SKIP_BUILD_ENV, "");

	if super::color_output_enabled() {
		build_cmd.arg("--color=always");
	}

	build_cmd.arg("--profile");
	build_cmd.arg(blob_build_profile.name());

	if offline_build() {
		build_cmd.arg("--offline");
	}

	// Our executor currently only supports the WASM MVP feature set, however nowadays
	// when compiling WASM the Rust compiler has more features enabled by default.
	//
	// We do set the `-C target-cpu=mvp` flag to make sure that *our* code gets compiled
	// in a way that is compatible with our executor, however this doesn't affect Rust's
	// standard library crates (`std`, `core` and `alloc`) which are by default precompiled
	// and still can make use of these extra features.
	//
	// So here we force the compiler to also compile the standard library crates for us
	// to make sure that they also only use the MVP features.
	if crate::build_std_required() {
		// Unfortunately this is still a nightly-only flag, but FWIW it is pretty widely used
		// so it's unlikely to break without a replacement.
		build_cmd.arg("-Z").arg("build-std");
		if !cargo_cmd.supports_nightly_features() {
			build_cmd.env("RUSTC_BOOTSTRAP", "1");
		}
	}

	println!("{}", colorize_info_message("Information that should be included in a bug report."));
	println!("{} {:?}", colorize_info_message("Executing build command:"), build_cmd);
	println!("{} {}", colorize_info_message("Using rustc version:"), cargo_cmd.rustc_version());

	// Use `process::exit(1)` to have a clean error output.
	if build_cmd.status().map(|s| s.success()).is_err() {
		process::exit(1);
	}

	let blob_name = get_blob_name(target, &manifest_path);
	let target_directory = project
		.join("target")
		.join(target.rustc_target())
		.join(blob_build_profile.directory());
	match target {
		RuntimeTarget::Riscv => {
			let elf_path = target_directory.join(&blob_name);
			let elf_metadata = match elf_path.metadata() {
				Ok(path) => path,
				Err(error) =>
					panic!("internal error: couldn't read the metadata of {elf_path:?}: {error}"),
			};

			let polkavm_path = target_directory.join(format!("{}.polkavm", blob_name));
			if polkavm_path
				.metadata()
				.map(|polkavm_metadata| {
					polkavm_metadata.modified().unwrap() >= elf_metadata.modified().unwrap()
				})
				.unwrap_or(true)
			{
				let blob_bytes =
					std::fs::read(elf_path).expect("binary always exists after its built");

				let mut config = polkavm_linker::Config::default();
				config.set_strip(true); // TODO: This shouldn't always be done.

				let program = match polkavm_linker::program_from_elf(config, &blob_bytes) {
					Ok(program) => program,
					Err(error) => {
						println!("Failed to link the runtime blob; this is probably a bug!");
						println!("Linking error: {error}");
						process::exit(1);
					},
				};

				std::fs::write(&polkavm_path, program.as_bytes())
					.expect("writing the blob to a file always works");
			}

			polkavm_path
		},
		RuntimeTarget::Wasm => target_directory.join(format!("{}.wasm", blob_name)),
	}
}

fn compact_wasm(
	project: &Path,
	inner_profile: &Profile,
	cargo_manifest: &Path,
	out_name: &str,
) -> Option<WasmBinary> {
	let default_out_name = get_blob_name(RuntimeTarget::Wasm, cargo_manifest);
	let in_path = project
		.join("target/wasm32-unknown-unknown")
		.join(inner_profile.directory())
		.join(format!("{}.wasm", default_out_name));

	let wasm_compact_path = project.join(format!("{}.compact.wasm", out_name));
	let start = std::time::Instant::now();
	wasm_opt::OptimizationOptions::new_opt_level_0()
		.mvp_features_only()
		.debug_info(true)
		.add_pass(wasm_opt::Pass::StripDwarf)
		.run(&in_path, &wasm_compact_path)
		.expect("Failed to compact generated WASM binary.");
	println!(
		"{} {}",
		colorize_info_message("Compacted wasm in"),
		colorize_info_message(format!("{:?}", start.elapsed()).as_str())
	);
	Some(WasmBinary(wasm_compact_path))
}

fn copy_bloaty_blob(
	project: &Path,
	inner_profile: &Profile,
	in_name: &str,
	out_name: &str,
) -> WasmBinaryBloaty {
	let in_path = project
		.join("target/wasm32-unknown-unknown")
		.join(inner_profile.directory())
		.join(format!("{}.wasm", in_name));

	let bloaty_path = project.join(format!("{}.wasm", out_name));
	fs::copy(in_path, &bloaty_path).expect("Copying the bloaty file to the project dir.");
	WasmBinaryBloaty(bloaty_path)
}

fn try_compress_blob(compact_blob_path: &Path, out_name: &str) -> Option<WasmBinary> {
	use sp_maybe_compressed_blob::CODE_BLOB_BOMB_LIMIT;

	let project = compact_blob_path.parent().expect("blob path should have a parent directory");
	let compact_compressed_blob_path =
		project.join(format!("{}.compact.compressed.wasm", out_name));

	let start = std::time::Instant::now();
	let data = fs::read(compact_blob_path).expect("Failed to read WASM binary");
	if let Some(compressed) = sp_maybe_compressed_blob::compress(&data, CODE_BLOB_BOMB_LIMIT) {
		fs::write(&compact_compressed_blob_path, &compressed[..])
			.expect("Failed to write WASM binary");

		println!(
			"{} {}",
			colorize_info_message("Compressed blob in"),
			colorize_info_message(format!("{:?}", start.elapsed()).as_str())
		);
		Some(WasmBinary(compact_compressed_blob_path))
	} else {
		build_helper::warning!(
			"Writing uncompressed blob. Exceeded maximum size {}",
			CODE_BLOB_BOMB_LIMIT,
		);
		println!("{}", colorize_info_message("Skipping blob compression"));
		None
	}
}

/// Custom wrapper for a [`cargo_metadata::Package`] to store it in
/// a `HashSet`.
#[derive(Debug)]
struct DeduplicatePackage<'a> {
	package: &'a cargo_metadata::Package,
	identifier: String,
}

impl<'a> From<&'a cargo_metadata::Package> for DeduplicatePackage<'a> {
	fn from(package: &'a cargo_metadata::Package) -> Self {
		Self {
			package,
			identifier: format!("{}{}{:?}", package.name, package.version, package.source),
		}
	}
}

impl<'a> Hash for DeduplicatePackage<'a> {
	fn hash<H: Hasher>(&self, state: &mut H) {
		self.identifier.hash(state);
	}
}

impl<'a> PartialEq for DeduplicatePackage<'a> {
	fn eq(&self, other: &Self) -> bool {
		self.identifier == other.identifier
	}
}

impl<'a> Eq for DeduplicatePackage<'a> {}

impl<'a> Deref for DeduplicatePackage<'a> {
	type Target = cargo_metadata::Package;

	fn deref(&self) -> &Self::Target {
		self.package
	}
}

fn create_metadata_command(path: impl Into<PathBuf>) -> MetadataCommand {
	let mut metadata_command = MetadataCommand::new();
	metadata_command.manifest_path(path);

	if offline_build() {
		metadata_command.other_options(vec!["--offline".to_owned()]);
	}
	metadata_command
}

/// Generate the `rerun-if-changed` instructions for cargo to make sure that the WASM binary is
/// rebuilt when needed.
fn generate_rerun_if_changed_instructions(
	cargo_manifest: &Path,
	project_folder: &Path,
	wasm_workspace: &Path,
	compressed_or_compact_wasm: Option<&WasmBinary>,
	bloaty_wasm: &WasmBinaryBloaty,
) {
	// Rerun `build.rs` if the `Cargo.lock` changes
	if let Some(cargo_lock) = find_cargo_lock(cargo_manifest) {
		rerun_if_changed(cargo_lock);
	}

	let metadata = create_metadata_command(project_folder.join("Cargo.toml"))
		.exec()
		.expect("`cargo metadata` can not fail!");

	let package = metadata
		.packages
		.iter()
		.find(|p| p.manifest_path == cargo_manifest)
		.expect("The crate package is contained in its own metadata; qed");

	// Start with the dependencies of the crate we want to compile for wasm.
	let mut dependencies = package.dependencies.iter().collect::<Vec<_>>();

	// Collect all packages by follow the dependencies of all packages we find.
	let mut packages = HashSet::new();
	packages.insert(DeduplicatePackage::from(package));

	while let Some(dependency) = dependencies.pop() {
		// Ignore all dev dependencies
		if dependency.kind == DependencyKind::Development {
			continue
		}

		let path_or_git_dep =
			dependency.source.as_ref().map(|s| s.starts_with("git+")).unwrap_or(true);

		let package = metadata
			.packages
			.iter()
			.filter(|p| !p.manifest_path.starts_with(wasm_workspace))
			.find(|p| {
				// Check that the name matches and that the version matches or this is
				// a git or path dep. A git or path dependency can only occur once, so we don't
				// need to check the version.
				(path_or_git_dep || dependency.req.matches(&p.version)) && dependency.name == p.name
			});

		if let Some(package) = package {
			if packages.insert(DeduplicatePackage::from(package)) {
				dependencies.extend(package.dependencies.iter());
			}
		}
	}

	// Make sure that if any file/folder of a dependency change, we need to rerun the `build.rs`
	packages.iter().for_each(package_rerun_if_changed);

	compressed_or_compact_wasm.map(|w| rerun_if_changed(w.wasm_binary_path()));
	rerun_if_changed(bloaty_wasm.bloaty_path());

	// Register our env variables
	println!("cargo:rerun-if-env-changed={}", crate::SKIP_BUILD_ENV);
	println!("cargo:rerun-if-env-changed={}", crate::WASM_BUILD_TYPE_ENV);
	println!("cargo:rerun-if-env-changed={}", crate::WASM_BUILD_RUSTFLAGS_ENV);
	println!("cargo:rerun-if-env-changed={}", crate::WASM_TARGET_DIRECTORY);
	println!("cargo:rerun-if-env-changed={}", crate::WASM_BUILD_TOOLCHAIN);
	println!("cargo:rerun-if-env-changed={}", crate::WASM_BUILD_STD);
	println!("cargo:rerun-if-env-changed={}", crate::RUNTIME_TARGET);
}

/// Track files and paths related to the given package to rerun `build.rs` on any relevant change.
fn package_rerun_if_changed(package: &DeduplicatePackage) {
	let mut manifest_path = package.manifest_path.clone();
	if manifest_path.ends_with("Cargo.toml") {
		manifest_path.pop();
	}

	WalkDir::new(&manifest_path)
		.into_iter()
		.filter_entry(|p| {
			// Ignore this entry if it is a directory that contains a `Cargo.toml` that is not the
			// `Cargo.toml` related to the current package. This is done to ignore sub-crates of a
			// crate. If such a sub-crate is a dependency, it will be processed independently
			// anyway.
			p.path() == manifest_path || !p.path().is_dir() || !p.path().join("Cargo.toml").exists()
		})
		.filter_map(|p| p.ok().map(|p| p.into_path()))
		.filter(|p| p.extension().map(|e| e == "rs" || e == "toml").unwrap_or_default())
		.for_each(rerun_if_changed);
}

/// Copy the blob binary to the target directory set in `WASM_TARGET_DIRECTORY` environment
/// variable. If the variable is not set, this is a no-op.
fn copy_blob_to_target_directory(cargo_manifest: &Path, blob_binary: &WasmBinary) {
	let target_dir = match env::var(crate::WASM_TARGET_DIRECTORY) {
		Ok(path) => PathBuf::from(path),
		Err(_) => return,
	};

	if !target_dir.is_absolute() {
		// We use println! + exit instead of a panic in order to have a cleaner output.
		println!(
			"Environment variable `{}` with `{}` is not an absolute path!",
			crate::WASM_TARGET_DIRECTORY,
			target_dir.display(),
		);
		process::exit(1);
	}

	fs::create_dir_all(&target_dir).expect("Creates `WASM_TARGET_DIRECTORY`.");

	fs::copy(
		blob_binary.wasm_binary_path(),
		target_dir.join(format!("{}.wasm", get_blob_name(RuntimeTarget::Wasm, cargo_manifest))),
	)
	.expect("Copies blob binary to `WASM_TARGET_DIRECTORY`.");
}
