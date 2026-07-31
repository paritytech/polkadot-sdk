// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

use crate::{mock::*, Error, SourceGenesis, SourceInfoOf};
use cumulus_primitives_core::ParaId;
use frame_support::{assert_noop, assert_ok};
use sp_runtime::DispatchError;

fn info() -> SourceInfoOf {
	([7u8; 32], None)
}

#[test]
fn set_updates_storage_and_the_api_view() {
	new_test_ext().execute_with(|| {
		let src = ParaId::from(2001u32);
		assert_ok!(SourceDiscovery::set_source_genesis(RuntimeOrigin::root(), src, Some(info()),));
		assert_eq!(SourceGenesis::<Test>::get(src), Some(info()));
		assert_eq!(SourceDiscovery::source_discovery_info(), vec![(src, ([7u8; 32], None))]);
	});
}

#[test]
fn clearing_with_none_removes_the_entry() {
	new_test_ext().execute_with(|| {
		let src = ParaId::from(2001u32);
		assert_ok!(SourceDiscovery::set_source_genesis(RuntimeOrigin::root(), src, Some(info())));
		assert_ok!(SourceDiscovery::set_source_genesis(RuntimeOrigin::root(), src, None));
		assert!(SourceGenesis::<Test>::get(src).is_none());
		assert!(SourceDiscovery::source_discovery_info().is_empty());
	});
}

#[test]
fn rejects_non_governance_origin() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			SourceDiscovery::set_source_genesis(
				RuntimeOrigin::signed(1),
				ParaId::from(2001u32),
				Some(info()),
			),
			DispatchError::BadOrigin,
		);
	});
}

#[test]
fn rejects_configuring_self_as_source() {
	new_test_ext().execute_with(|| {
		// `SelfParaId` in the mock is 2000.
		assert_noop!(
			SourceDiscovery::set_source_genesis(
				RuntimeOrigin::root(),
				ParaId::from(2000u32),
				Some(info()),
			),
			Error::<Test>::SelfSource,
		);
	});
}

#[test]
fn caps_new_sources_but_allows_updates_at_capacity() {
	new_test_ext().execute_with(|| {
		// `MaxSources` in the mock is 2.
		let (a, b, c) = (ParaId::from(2001u32), ParaId::from(2002u32), ParaId::from(2003u32));
		assert_ok!(SourceDiscovery::set_source_genesis(RuntimeOrigin::root(), a, Some(info())));
		assert_ok!(SourceDiscovery::set_source_genesis(RuntimeOrigin::root(), b, Some(info())));

		// A new source beyond the cap is rejected …
		assert_noop!(
			SourceDiscovery::set_source_genesis(RuntimeOrigin::root(), c, Some(info())),
			Error::<Test>::TooManySources,
		);
		// … updating an existing source at capacity is allowed …
		assert_ok!(SourceDiscovery::set_source_genesis(
			RuntimeOrigin::root(),
			a,
			Some(([9u8; 32], None)),
		));
		assert_eq!(SourceGenesis::<Test>::get(a), Some(([9u8; 32], None)));

		// … and removing one frees a slot for a new source.
		assert_ok!(SourceDiscovery::set_source_genesis(RuntimeOrigin::root(), b, None));
		assert_ok!(SourceDiscovery::set_source_genesis(RuntimeOrigin::root(), c, Some(info())));
	});
}
