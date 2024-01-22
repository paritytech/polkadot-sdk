//! # Development Environment Advice
//!
//! Large Rust projects are known for sometimes long compile times and sluggish dev tooling, and
//! polkadot-sdk is no exception.
//!
//! This page contains some advice to improve your workflow when using common tooling.
//!
//! ## Rust Analyzer Configuration
//!
//! [Rust Analyzer](https://rust-analyzer.github.io/) is the defacto [LSP](https://langserver.org/) for Rust. Its default
//! settings are fine for smaller projects, but not well optimised for polkadot-sdk.
//!
//! Below is a suggested configuration for VSCode:
//!
//! ```json
//! {
//!   // Use a separate target dir for Rust Analyzer. Helpful if you want to use Rust
//!   // Analyzer and cargo on the command line at the same time.
//!   "rust-analyzer.rust.analyzerTargetDir": "target/vscode-rust-analyzer",
//!   // Improve stability
//!   "rust-analyzer.server.extraEnv": {
//!     "CHALK_OVERFLOW_DEPTH": "100000000",
//!     "CHALK_SOLVER_MAX_SIZE": "10000000"
//!   },
//!   // Check feature-gated code
//!   "rust-analyzer.cargo.features": "all",
//!   "rust-analyzer.cargo.extraEnv": {
//!     // Skip building WASM, there is never need for it here
//!     "SKIP_WASM_BUILD": "1"
//!   },
//!   // Don't expand some problematic proc_macros
//!   "rust-analyzer.procMacro.ignored": {
//!     "async-trait": ["async_trait"],
//!     "napi-derive": ["napi"],
//!     "async-recursion": ["async_recursion"],
//!     "async-std": ["async_std"]
//!   },
//!   // Use nightly formatting.
//!   // See the polkadot-sdk CI job that checks formatting for the current version used in
//!   // polkadot-sdk.
//!   "rust-analyzer.rustfmt.extraArgs": ["+nightly-2024-01-22"],
//! }
//! ```
//!
//! and the same in Lua for `neovim/nvim-lspconfig`:
//!
//! ```lua
//! ["rust-analyzer"] = {
//!   rust = {
//!     # Use a separate target dir for Rust Analyzer. Helpful if you want to use Rust
//!     # Analyzer and cargo on the command line at the same time.
//!     analyzerTargetDir = "target/nvim-rust-analyzer",
//!   },
//!   server = {
//!     # Improve stability
//!     extraEnv = {
//!       ["CHALK_OVERFLOW_DEPTH"] = "100000000",
//!       ["CHALK_SOLVER_MAX_SIZE"] = "100000000",
//!     },
//!   },
//!   cargo = {
//!     # Check feature-gated code
//!     features = "all",
//!     extraEnv = {
//!       # Skip building WASM, there is never need for it here
//!       ["SKIP_WASM_BUILD"] = "1",
//!     },
//!   },
//!   procMacro = {
//!     # Don't expand some problematic proc_macros
//!     ignored = {
//!       ["async-trait"] = { "async_trait" },
//!       ["napi-derive"] = { "napi" },
//!       ["async-recursion"] = { "async_recursion" },
//!       ["async-std"] = { "async_std" },
//!     },
//!   },
//!   rustfmt = {
//!     # Use nightly formatting.
//!     # See the polkadot-sdk CI job that checks formatting for the current version used in
//!     # polkadot-sdk.
//!     extraArgs = { "+nightly-2024-01-22" },
//!   },
//! },
//! ```
//!
//! For the full set of configuration options see <https://rust-analyzer.github.io/manual.html#configuration>.
//!
//! ## Cargo Usage
//!
//! ### Using `--package` (a.k.a. `-p`)
//!
//! polkadot-sdk is a monorepo containing many crates. When you run a cargo command without
//! `-p`, you will almost certainly compile crates outside of the scope you are working.
//!
//! Instead, you should identify the name of the crate you are working on by checking the `name`
//! field in the closest `Cargo.toml` file. Then, use `-p` with your cargo commands to only compile
//! that crate.
//!
//! ### `SKIP_WASM_BUILD=1` environment variable
//!
//! When cargo touches a runtime crate, by default it will also compile the WASM binary,
//! approximately doubling the compilation time.
//!
//! The WASM binary is usually not needed, especially when running `check` or `test`. To skip the
//! WASM build, set the `SKIP_WASM_BUILD` environment variable to `1`. For example:
//! `SKIP_WASM_BUILD=1 cargo check -p frame-support`.
//!
//! ### Cargo Remote
//!
//! If you have a powerful remote server available, you may consider using
//! [cargo-remote](https://github.com/sgeisler/cargo-remote) to execute cargo commands on it,
//! freeing up local resources for other tasks like `rust-analyzer`.
