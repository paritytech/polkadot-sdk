# Project Context

## Purpose

This is polkadot-sdk, a SDK for writing blockchains in Rust. It is used by Polkadot itself and for writing parachains that are running on top of it.

## Tech Stack

- Rust

## Project Conventions

### Code Style

- Use `cargo fmt --all` to format Rust code
- Use `taplo fmt` for formatting `Cargo.toml` files
- Keep comments short and on point - do not comment obvious things
- When copying code, ensure you copy it literally without forgetting or changing anything

### Testing Strategy

- When fixing bugs, verify fixes with the compiler/tests - whatever initially reported the bug/issue
- Bug fixes should be minimal - NEVER refactor while fixing

