// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use snowbridge_core::sparse_bitmap::SparseBitmapImpl;

pub type Nonce<T> = SparseBitmapImpl<crate::NonceBitmap<T>>;
