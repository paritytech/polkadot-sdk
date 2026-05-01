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

//! Type-level helpers for extracting execution-facing storage types.
//!
//! FRAME storage aliases such as `AccountInfoOf<T>` hide the concrete `StorageValue` or
//! `StorageMap` instantiation behind generated types. That is useful at call sites, but it makes it
//! awkward to name the key or value exposed by a concrete storage item when building wrappers
//! around storage codec types.
//!
//! This module provides small extraction traits and aliases for those cases. They do not read or
//! write storage. They only answer type-system questions such as "what value does this
//! `StorageValue` expose?" or "what key and value does this `StorageMap` use?".

use frame_support::pallet_prelude::{StorageMap, StorageValue};

/// The value type exposed by a concrete [`StorageValue`] definition.
///
/// Use this alias when code needs to refer to the value returned by a storage value without
/// repeating the storage declaration's generic parameters by hand.
///
/// [`StorageValue`]: frame_support::pallet_prelude::StorageValue
pub type StorageValueOf<T> = <T as ValueContainerOfStorageValue>::Value;

/// The key type accepted by a concrete [`StorageMap`] definition.
///
/// This is useful when a helper is generic over a storage map and needs to name the map's key type
/// independently from the value type stored under that key.
///
/// [`StorageMap`]: frame_support::pallet_prelude::StorageMap
pub type StorageMapKeyOf<T> = <T as KeyValueContainerOfStorageMap>::Key;

/// The value type exposed by a concrete [`StorageMap`] definition.
///
/// Use this alias when code needs to name the value returned by a storage map while still deriving
/// that type from the map declaration itself.
///
/// [`StorageMap`]: frame_support::pallet_prelude::StorageMap
pub type StorageMapValueOf<T> = <T as KeyValueContainerOfStorageMap>::Value;

/// Extracts the value type from a [`StorageValue`] definition.
///
/// Implementations are provided for FRAME's storage value type. The trait exists so aliases such as
/// [`StorageValueOf`] can ask for the exposed value type in a uniform way.
///
/// [`StorageValue`]: frame_support::pallet_prelude::StorageValue
pub trait ValueContainerOfStorageValue {
	/// The value type exposed by the storage value.
	type Value;
}

/// Extracts the key and value types from a [`StorageMap`] definition.
///
/// Implementations are provided for FRAME's storage map type. The trait exists so aliases such as
/// [`StorageMapKeyOf`] and [`StorageMapValueOf`] can name map components without duplicating the
/// map declaration's generic parameters.
///
/// [`StorageMap`]: frame_support::pallet_prelude::StorageMap
pub trait KeyValueContainerOfStorageMap {
	/// The key type used to address entries in the storage map.
	type Key;

	/// The value type exposed by the storage map for each key.
	type Value;
}

impl<Prefix, Value, QueryKind, OnEmpty> ValueContainerOfStorageValue
	for StorageValue<Prefix, Value, QueryKind, OnEmpty>
{
	type Value = Value;
}

impl<Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues> KeyValueContainerOfStorageMap
	for StorageMap<Prefix, Hasher, Key, Value, QueryKind, OnEmpty, MaxValues>
{
	type Key = Key;
	type Value = Value;
}
