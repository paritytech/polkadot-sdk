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
//! Below is a suggested configuration in Lua (also valid `neovim/nvim-lspconfig` configuration)
//! with comments describing the rationale for each setting:
//!
//! ```lua
//! ["rust-analyzer"] = {
//!   cargo = {
//!     # Check feature-gated code blocks
//!     features = "all",
//!     extraEnv = {
//!       # Use a seperate target dir for Rust Analyzer. Helpful if you wish to use Rust
//!       # Analyzer and cargo on the command line at the same time.
//!       ["CARGO_TARGET_DIR"] = "target/nvim-rust-analyzer",
//!       # Skip the WASM build
//!       ["SKIP_WASM_BUILD"] = "1",
//!       # Reduce chance of random crashes
//!       ["CHALK_OVERFLOW_DEPTH"] = "100000000",
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
//!     # Use a nightly version for auto formatting.
//!     # See our CI 'check format' job for the version currently in use.
//!     extraArgs = { "+nightly-2023-11-01" },
//!   },
//! },
//! ```
//!
//! and the same configuration in JSON for VSCode's `settings.json`:
//!
//! ```json
//! {
//!   "rust-analyzer.cargo.features": "all",
//!   "rust-analyzer.cargo.extraEnv": {
//!     "CARGO_TARGET_DIR": "target/vscode-rust-analyzer",
//!     "SKIP_WASM_BUILD": "1",
//!     "CHALK_OVERFLOW_DEPTH": "100000000"
//!   },
//!   "rust-analyzer.procMacro.ignored": {
//!     "async-trait": ["async_trait"],
//!     "napi-derive": ["napi"],
//!     "async-recursion": ["async_recursion"],
//!     "async-std": ["async_std"]
//!   },
//!   "rust-analyzer.rustfmt.extraArgs": ["+nightly-2023-11-01"],
//! }
//! ```
//!
//! For the full set of configuation options see <https://rust-analyzer.github.io/manual.html#configuration>.
//!
//! ## Cargo Usage
//!
//! ### Using `--package` (a.k.a. `-p`)
//!
//! polkadot-sdk is a monorepo containing many crates. When you run a cargo command without
//! `-p`, you will almost certinally compile crates outside of the scope you are working on.
//!
//! Instead, you should identify the name of the crate you are working on by checking them `name`
//! field in the closest `Cargo.toml` file. Then, use `-p` with your cargo commands to only compile
//! that crate.
//!
//! ### `SKIP_WASM_BUILD=1` environment variable
//!
//! When cargo touches a runtime crate, by default it will also compile the WASM binary,
//! approximately doubling the compilation time.
//!
//! The WASM binary is almost never needed, especially when running `check` or `test`. To skip the
//! WASM build, set the `SKIP_WASM_BUILD` environment variable to `1`. For example:
//! `SKIP_WASM_BUILD=1 cargo check -p frame-support`.
//!
//! ### Cargo Remote
//!
//! If you have a beefy remote server avaliable, you may consider using [cargo-remote](https://github.com/sgeisler/cargo-remote) to simply offload
//! cargo commands to it, freeing up local resources for other tasks like `rust-analyzer`.
