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

//! Tests for pallet-delegated-staking.

use super::*;
use crate::{mock::*, Event};
#[test]
fn create_a_delegatee_with_first_delegator() {
	// Similar to creating a nomination pool
	ExtBuilder::default().build_and_execute(|| assert!(true));
}

#[test]
fn add_delegation_to_existing_delegator() {
	// Similar to creating a nomination pool
	ExtBuilder::default().build_and_execute(|| assert!(true));
}

#[test]
fn create_multiple_delegators() {
	// Similar to creating a nomination pool
	ExtBuilder::default().build_and_execute(|| assert!(true));
}

#[test]
fn withdraw_delegation() {
	// Similar to creating a nomination pool
	ExtBuilder::default().build_and_execute(|| assert!(true));
}

#[test]
fn apply_pending_slash() {
	// Similar to creating a nomination pool
	ExtBuilder::default().build_and_execute(|| assert!(true));
}

#[test]
fn distribute_rewards() {
	// Similar to creating a nomination pool
	ExtBuilder::default().build_and_execute(|| assert!(true));
}

#[test]
fn migrate_to_delegator() {
	// Similar to creating a nomination pool
	ExtBuilder::default().build_and_execute(|| assert!(true));
}
