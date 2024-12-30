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
//! Helper functions for OPF pallet.


pub use super::*;
impl<T: Config> Pallet<T> {

    // Helper function for project registration
    pub fn register_new(project_id: ProjectId<T>, amount: BalanceOf<T>) -> DispatchResult{
        let submission_block = T::BlockNumberProvider::current_block_number();
        let project_infos: ProjectInfo<T> = ProjectInfo { project_id, submission_block, amount};
        let mut bounded = Projects::get();
        let _ = bounded.try_push(project_infos);
        Projects::put(bounded);
        Ok(())
    }
}