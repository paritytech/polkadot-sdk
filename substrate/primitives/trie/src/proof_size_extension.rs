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

//! Externalities extension that provides access to the current proof size
//! of the underlying recorder.

use crate::ProofSizeProvider;

sp_externalities::decl_extension! {
	/// The proof size extension to fetch the current storage proof size
	/// in externalities.
	pub struct ProofSizeExt(Box<dyn ProofSizeProvider + 'static + Sync + Send>);
}

impl ProofSizeExt {
	/// Creates a new instance of [`ProofSizeExt`].
	pub fn new<T: ProofSizeProvider + Sync + Send + 'static>(recorder: T) -> Self {
		ProofSizeExt(Box::new(recorder))
	}

	/// Returns the storage proof size.
	pub fn storage_proof_size(&self) -> u64 {
		self.0.estimate_encoded_size() as _
	}
}
