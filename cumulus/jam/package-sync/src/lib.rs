// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! Generic, bytes-only networking for JAM collator-to-collator work-package sync.
//!
//! A collator that imports a block authored by another collator cannot recompute that block's
//! work-package hash: the package contains the author's randomised signature (`authorization`).
//! This crate implements the request/response protocol a collator uses to learn, per block hash,
//! the information it cannot derive from the imported block: [`types::PackageInfo`] — the
//! author's `authorization`, `prerequisites` and PoV [`types::PovSpec`].
//!
//! The crate speaks bytes only: it has no dependency on `jam-types` or any `parachain-*` path
//! crate. The JAM-specific glue (rebuilding and verifying the package) is implemented by the
//! caller through [`handler::PackageInfoProvider`], [`fetcher::PeerTargets`] and
//! [`fetcher::PackageInfoAcceptor`].

pub mod fetcher;
pub mod handler;
pub mod protocol;
pub mod store;
pub mod types;
