## WASM builder is a utility for building a project as a WASM binary

The WASM builder is a tool that integrates the process of building the WASM binary of your project into the main
`cargo` build process.

### Project setup

A project that should be compiled as a WASM binary needs to:

1. Add a `build.rs` file.
2. Add `substrate-wasm-builder-runner` as dependency into `build-dependencies`.
3. Add a feature called `no-std`.

The `build.rs` file needs to contain the following code:

```rust
use wasm_builder_runner::{build_current_project, WasmBuilderSource};

fn main() {
	build_current_project(
		// The name of the file being generated in out-dir.
		"wasm_binary.rs",
		// How to include wasm-builder, in this case from crates.io.
		WasmBuilderSource::Crates("1.0.0"),
	);
}
```

The `no-std` feature will be enabled by WASM builder while compiling your project to WASM.

As the final step, you need to add the following to your project:

```rust
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
```

This will include the generated WASM binary as two constants `WASM_BINARY` and `WASM_BINARY_BLOATY`.
The former is a compact WASM binary and the latter is not compacted.

### Environment variables

By using environment variables, you can configure which WASM binaries are built and how:

- `SKIP_WASM_BUILD` - Skips building any WASM binary. This is useful when only native should be recompiled.
- `BUILD_DUMMY_WASM_BINARY` - Builds dummy WASM binaries. These dummy binaries are empty and useful
                             for `cargo check` runs.
- `WASM_BUILD_TYPE` - Sets the build type for building WASM binaries. Supported values are `release` or `debug`.
                      By default the build type is equal to the build type used by the main build.
- `TRIGGER_WASM_BUILD` - Can be set to trigger a WASM build. On subsequent calls the value of the variable
                         needs to change. As WASM builder instructs `cargo` to watch for file changes
                         this environment variable should only be required in certain circumstances.

Each project can be skipped individually by using the environment variable `SKIP_PROJECT_NAME_WASM_BUILD`.
Where `PROJECT_NAME` needs to be replaced by the name of the cargo project, e.g. `node-runtime` will
be `NODE_RUNTIME`.

### Prerequisites:

WASM builder requires the following prerequisities for building the WASM binary:

- rust nightly + `wasm32-unknown-unknown` toolchain
- wasm-gc


License: GPL-3.0
