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

/// Get the current branch and commit hash.
fn main() {
    let repo = git2::Repository::open("../../../..").expect("should be a repository");
    let head = repo.head().expect("should have head");
    let commit = head.peel_to_commit().expect("should have commit");
	let branch = head.shorthand().unwrap_or("unknown").to_string();
    let id = &commit.id().to_string()[..7];
	println!("cargo:rustc-env=GIT_BRANCH_NAME={branch}");
    println!("cargo:rustc-env=GIT_COMMIT_HASH={id}");
}
