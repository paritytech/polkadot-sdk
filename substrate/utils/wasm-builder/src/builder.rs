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

use std::{
	env,
	path::{Path, PathBuf},
	process,
};

use crate::{wasm_project::BuiltBlob, RuntimeTarget};

/// Extra information when generating the `metadata-hash`.
#[cfg(feature = "metadata-hash")]
pub(crate) struct MetadataExtraInfo {
	pub decimals: u8,
	pub token_symbol: String,
}

/// The `metadata-hash` that should be compiled into the blob.
#[cfg(feature = "metadata-hash")]
pub(crate) enum MetadataHash {
	/// Generate the hash by building the blob twice.
	Generate(MetadataExtraInfo),
	/// Use the hash that was already generated for another target.
	Reuse([u8; 32]),
}

/// Returns the manifest dir from the `CARGO_MANIFEST_DIR` env.
fn get_manifest_dir() -> PathBuf {
	env::var("CARGO_MANIFEST_DIR")
		.expect("`CARGO_MANIFEST_DIR` is always set for `build.rs` files; qed")
		.into()
}

/// First step of the [`WasmBuilder`] to select the project to build.
pub struct WasmBuilderSelectProject {
	/// This parameter just exists to make it impossible to construct
	/// this type outside of this crate.
	_ignore: (),
}

impl WasmBuilderSelectProject {
	/// Use the current project as project for building the WASM binary.
	///
	/// # Panics
	///
	/// Panics if the `CARGO_MANIFEST_DIR` variable is not set. This variable
	/// is always set by `Cargo` in `build.rs` files.
	pub fn with_current_project(self) -> WasmBuilder {
		WasmBuilder {
			rust_flags: Vec::new(),
			file_name: None,
			project_cargo_toml: get_manifest_dir().join("Cargo.toml"),
			features_to_enable: Vec::new(),
			disable_runtime_version_section_check: false,
			export_heap_base: false,
			import_memory: false,
			enable_pvm: false,
			#[cfg(feature = "metadata-hash")]
			enable_metadata_hash: None,
		}
	}

	/// Use the given `path` as project for building the WASM binary.
	///
	/// Returns an error if the given `path` does not points to a `Cargo.toml`.
	pub fn with_project(self, path: impl Into<PathBuf>) -> Result<WasmBuilder, &'static str> {
		let path = path.into();

		if path.ends_with("Cargo.toml") && path.exists() {
			Ok(WasmBuilder {
				rust_flags: Vec::new(),
				file_name: None,
				project_cargo_toml: path,
				features_to_enable: Vec::new(),
				disable_runtime_version_section_check: false,
				export_heap_base: false,
				import_memory: false,
				enable_pvm: false,
				#[cfg(feature = "metadata-hash")]
				enable_metadata_hash: None,
			})
		} else {
			Err("Project path must point to the `Cargo.toml` of the project")
		}
	}
}

/// The builder for building a wasm binary.
///
/// The builder itself is separated into multiple structs to make the setup type safe.
///
/// Building a wasm binary:
///
/// 1. Call [`WasmBuilder::new`] to create a new builder.
/// 2. Select the project to build using the methods of [`WasmBuilderSelectProject`].
/// 3. Set additional `RUST_FLAGS` or a different name for the file containing the WASM code using
///    methods of [`WasmBuilder`].
/// 4. Build the WASM binary using [`Self::build`].
pub struct WasmBuilder {
	/// Flags that should be appended to `RUST_FLAGS` env variable.
	rust_flags: Vec<String>,
	/// The name of the file that is being generated in `OUT_DIR`.
	///
	/// Defaults to `wasm_binary.rs`.
	file_name: Option<String>,
	/// The path to the `Cargo.toml` of the project that should be built
	/// for wasm.
	project_cargo_toml: PathBuf,
	/// Features that should be enabled when building the wasm binary.
	features_to_enable: Vec<String>,
	/// Should the builder not check that the `runtime_version` section exists in the wasm binary?
	disable_runtime_version_section_check: bool,

	/// Whether `__heap_base` should be exported (WASM-only).
	export_heap_base: bool,
	/// Whether `--import-memory` should be added to the link args (WASM-only).
	import_memory: bool,

	/// Whether the PVM binary should be built as well.
	enable_pvm: bool,

	/// Whether to enable the metadata hash generation.
	#[cfg(feature = "metadata-hash")]
	enable_metadata_hash: Option<MetadataExtraInfo>,
}

impl WasmBuilder {
	/// Create a new instance of the builder.
	pub fn new() -> WasmBuilderSelectProject {
		WasmBuilderSelectProject { _ignore: () }
	}

	/// Build the WASM binary using the recommended default values.
	///
	/// This is the same as calling:
	/// ```no_run
	/// substrate_wasm_builder::WasmBuilder::new()
	///    .with_current_project()
	///    .import_memory()
	///    .export_heap_base()
	///    .build();
	/// ```
	pub fn build_using_defaults() {
		WasmBuilder::new()
			.with_current_project()
			.import_memory()
			.export_heap_base()
			.build();
	}

	/// Init the wasm builder with the recommended default values.
	///
	/// In contrast to [`Self::build_using_defaults`] it does not build the WASM binary directly.
	///
	/// This is the same as calling:
	/// ```no_run
	/// substrate_wasm_builder::WasmBuilder::new()
	///    .with_current_project()
	///    .import_memory()
	///    .export_heap_base();
	/// ```
	pub fn init_with_defaults() -> Self {
		WasmBuilder::new().with_current_project().import_memory().export_heap_base()
	}

	/// Enable exporting `__heap_base` as global variable in the WASM binary.
	///
	/// This adds `-C link-arg=--export=__heap_base` to `RUST_FLAGS`.
	pub fn export_heap_base(mut self) -> Self {
		self.export_heap_base = true;
		self
	}

	/// Set the name of the file that will be generated in `OUT_DIR`.
	///
	/// This file needs to be included to get access to the build WASM binary.
	///
	/// If this function is not called, `file_name` defaults to `wasm_binary.rs`
	pub fn set_file_name(mut self, file_name: impl Into<String>) -> Self {
		self.file_name = Some(file_name.into());
		self
	}

	/// Instruct the linker to import the memory into the WASM binary.
	///
	/// This adds `-C link-arg=--import-memory` to `RUST_FLAGS`.
	pub fn import_memory(mut self) -> Self {
		self.import_memory = true;
		self
	}

	/// Build the PVM binary next to the WASM binary.
	///
	/// The generated file will contain the extra constants `PVM_BINARY` and `PVM_BINARY_PATH`
	/// pointing to the PolkaVM program.
	///
	/// If `SUBSTRATE_RUNTIME_TARGET` selects the `riscv` target, only the PVM binary is built and
	/// the `PVM_BINARY*` constants alias the `WASM_BINARY*` constants.
	pub fn enable_pvm(mut self) -> Self {
		self.enable_pvm = true;
		self
	}

	/// Append the given `flag` to `RUST_FLAGS`.
	///
	/// `flag` is appended as is, so it needs to be a valid flag.
	pub fn append_to_rust_flags(mut self, flag: impl Into<String>) -> Self {
		self.rust_flags.push(flag.into());
		self
	}

	/// Enable the given feature when building the wasm binary.
	///
	/// `feature` needs to be a valid feature that is defined in the project `Cargo.toml`.
	pub fn enable_feature(mut self, feature: impl Into<String>) -> Self {
		self.features_to_enable.push(feature.into());
		self
	}

	/// Enable generation of the metadata hash.
	///
	/// This will compile the runtime once, fetch the metadata, build the metadata hash and
	/// then compile again with the env `RUNTIME_METADATA_HASH` set. For more information
	/// about the metadata hash see [RFC78](https://polkadot-fellows.github.io/RFCs/approved/0078-merkleized-metadata.html).
	///
	/// - `token_symbol`: The symbol of the main native token of the chain.
	/// - `decimals`: The number of decimals of the main native token.
	#[cfg(feature = "metadata-hash")]
	pub fn enable_metadata_hash(mut self, token_symbol: impl Into<String>, decimals: u8) -> Self {
		self.enable_metadata_hash =
			Some(MetadataExtraInfo { token_symbol: token_symbol.into(), decimals });

		self
	}

	/// Disable the check for the `runtime_version` wasm section.
	///
	/// By default the `wasm-builder` will ensure that the `runtime_version` section will
	/// exists in the build wasm binary. This `runtime_version` section is used to get the
	/// `RuntimeVersion` without needing to call into the wasm binary. However, for some
	/// use cases (like tests) you may want to disable this check.
	pub fn disable_runtime_version_section_check(mut self) -> Self {
		self.disable_runtime_version_section_check = true;
		self
	}

	/// Returns the `RUSTFLAGS` to use for building the given `target`.
	fn rust_flags(&self, target: RuntimeTarget) -> String {
		let mut rust_flags = self.rust_flags.clone();

		if target == RuntimeTarget::Wasm {
			if self.export_heap_base {
				rust_flags.push("-C link-arg=--export=__heap_base".into());
			}

			if self.import_memory {
				rust_flags.push("-C link-arg=--import-memory".into());
			}
		}

		rust_flags.join(" ")
	}

	/// Build the WASM binary.
	pub fn build(self) {
		let target = RuntimeTarget::new();

		let out_dir = PathBuf::from(env::var("OUT_DIR").expect("`OUT_DIR` is set by cargo!"));
		let file_path =
			out_dir.join(self.file_name.clone().unwrap_or_else(|| "wasm_binary.rs".into()));

		if check_skip_build() {
			// If we skip the build, we still want to make sure to be called when an env variable
			// changes
			generate_rerun_if_changed_instructions();

			provide_dummy_wasm_binary_if_not_exist(&file_path, self.enable_pvm);

			return;
		}

		// If the target is already `riscv`, the blob built below is the PVM blob.
		let build_pvm = self.enable_pvm && target != RuntimeTarget::Riscv;
		let rust_flags = self.rust_flags(target);
		let pvm_rust_flags = self.rust_flags(RuntimeTarget::Riscv);

		let blob = build_blob(
			target,
			&self.project_cargo_toml,
			rust_flags,
			self.features_to_enable.clone(),
			self.file_name.clone(),
			!self.disable_runtime_version_section_check,
			#[cfg(feature = "metadata-hash")]
			self.enable_metadata_hash.map(MetadataHash::Generate),
		);

		let pvm_blob = build_pvm.then(|| {
			build_blob(
				RuntimeTarget::Riscv,
				&self.project_cargo_toml,
				pvm_rust_flags,
				self.features_to_enable,
				self.file_name,
				!self.disable_runtime_version_section_check,
				// Both blobs are built from the same runtime and thus, share the metadata hash.
				// Reusing it also means that we don't need to execute the PVM blob.
				#[cfg(feature = "metadata-hash")]
				blob.metadata_hash.map(MetadataHash::Reuse),
			)
		});

		write_blob_constants(&file_path, &blob, pvm_blob.as_ref(), self.enable_pvm);

		// As last step we need to generate our `rerun-if-changed` stuff. If a build fails, we don't
		// want to spam the output!
		generate_rerun_if_changed_instructions();
	}
}

/// Generate the name of the skip build environment variable for the current crate.
fn generate_crate_skip_build_env_name() -> String {
	format!(
		"SKIP_{}_WASM_BUILD",
		env::var("CARGO_PKG_NAME")
			.expect("Package name is set")
			.to_uppercase()
			.replace('-', "_"),
	)
}

/// Checks if the build of the WASM binary should be skipped.
fn check_skip_build() -> bool {
	env::var(crate::SKIP_BUILD_ENV).is_ok() ||
		env::var(generate_crate_skip_build_env_name()).is_ok() ||
		// If we are running in docs.rs, let's skip building.
		// https://docs.rs/about/builds#detecting-docsrs
		env::var("DOCS_RS").is_ok()
}

/// Provide a dummy WASM binary if there doesn't exist one.
fn provide_dummy_wasm_binary_if_not_exist(file_path: &Path, enable_pvm: bool) {
	if file_path.exists() {
		return;
	}

	let mut constants = String::from(
		"pub const WASM_BINARY_PATH: Option<&str> = None;\
		 pub const WASM_BINARY: Option<&[u8]> = None;\
		 pub const WASM_BINARY_BLOATY: Option<&[u8]> = None;",
	);

	if enable_pvm {
		constants.push_str(
			"pub const PVM_BINARY_PATH: Option<&str> = None;\
			 pub const PVM_BINARY: Option<&[u8]> = None;",
		);
	}

	crate::write_file_if_changed(file_path, constants);
}

/// Generate the `rerun-if-changed` instructions for cargo to make sure that the WASM binary is
/// rebuilt when needed.
fn generate_rerun_if_changed_instructions() {
	// Make sure that the `build.rs` is called again if one of the following env variables changes.
	println!("cargo:rerun-if-env-changed={}", crate::SKIP_BUILD_ENV);
	println!("cargo:rerun-if-env-changed={}", crate::FORCE_WASM_BUILD_ENV);
	println!("cargo:rerun-if-env-changed={}", generate_crate_skip_build_env_name());
}

/// Build the given project for `target`.
///
/// `project_cargo_toml` - The path to the `Cargo.toml` of the project that should be built.
///
/// `default_rustflags` - Default `RUSTFLAGS` that will always be set for the build.
///
/// `features_to_enable` - Features that should be enabled for the project.
///
/// `blob_name` - The optional blob name that is extended with `.compact.compressed.wasm`.
/// If `None`, the project name will be used.
///
/// `check_for_runtime_version_section` - Should the wasm binary be checked for the
/// `runtime_version` section?
fn build_blob(
	target: RuntimeTarget,
	project_cargo_toml: &Path,
	default_rustflags: String,
	features_to_enable: Vec<String>,
	blob_name: Option<String>,
	check_for_runtime_version_section: bool,
	#[cfg(feature = "metadata-hash")] metadata_hash: Option<MetadataHash>,
) -> BuiltBlob {
	// Init jobserver as soon as possible
	crate::wasm_project::get_jobserver();
	let cargo_cmd = match crate::prerequisites::check(target) {
		Ok(cmd) => cmd,
		Err(err_msg) => {
			eprintln!("{err_msg}");
			process::exit(1);
		},
	};

	crate::wasm_project::create_and_compile(
		target,
		project_cargo_toml,
		&default_rustflags,
		cargo_cmd,
		features_to_enable,
		blob_name,
		check_for_runtime_version_section,
		#[cfg(feature = "metadata-hash")]
		metadata_hash,
	)
}

/// Write the constants pointing to the built blobs into `file_path`.
///
/// `pvm_blob` is the PVM blob that was built next to the WASM blob. If `enable_pvm` is set, but
/// there is no separate `pvm_blob`, the WASM blob is the PVM blob and the constants are aliased.
fn write_blob_constants(
	file_path: &Path,
	blob: &BuiltBlob,
	pvm_blob: Option<&BuiltBlob>,
	enable_pvm: bool,
) {
	let wasm_binary = blob.final_path_escaped();
	let wasm_binary_bloaty = blob.bloaty.bloaty_path_escaped();

	let pvm_constants = match (pvm_blob, enable_pvm) {
		(Some(pvm_blob), _) => {
			let pvm_binary = pvm_blob.bloaty.bloaty_path_escaped();

			format!(
				r#"
				pub const PVM_BINARY_PATH: Option<&str> = Some("{pvm_binary}");
				pub const PVM_BINARY: Option<&[u8]> = Some(include_bytes!("{pvm_binary}"));
			"#
			)
		},
		(None, true) => r#"
				pub const PVM_BINARY_PATH: Option<&str> = WASM_BINARY_PATH;
				pub const PVM_BINARY: Option<&[u8]> = WASM_BINARY;
			"#
		.into(),
		(None, false) => String::new(),
	};

	crate::write_file_if_changed(
		file_path,
		format!(
			r#"
				pub const WASM_BINARY_PATH: Option<&str> = Some("{wasm_binary}");
				pub const WASM_BINARY: Option<&[u8]> = Some(include_bytes!("{wasm_binary}"));
				pub const WASM_BINARY_BLOATY: Option<&[u8]> = Some(include_bytes!("{wasm_binary_bloaty}"));
			{pvm_constants}"#
		),
	);
}
