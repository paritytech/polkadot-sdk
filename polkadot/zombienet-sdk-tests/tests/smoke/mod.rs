// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// TODO: It sets right feature name, because we don't use `zombie-metadata` anymore.
// But better to delete the change and fix it in the other PR.
// I use it here only to debug the collator metrics during PR development spawning zombinet test
// ZOMBIE_PROVIDER=native cargo test -p polkadot-zombienet-sdk-tests \
// --features zombie-ci smoke::coretime_revenue::coretime_revenue_test \
// -- --exact --nocapture
#[cfg(feature = "zombie-ci")]
mod coretime_revenue;
