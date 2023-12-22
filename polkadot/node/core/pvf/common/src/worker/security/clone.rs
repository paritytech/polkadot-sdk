// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Functionality for securing the job processes spawned by the workers using `clone`. If
//! unsupported, falls back to `fork`.

use nix::sched::CloneFlags;

/// Returns all the sandbox-related flags for `clone`.
pub fn clone_sandbox_flags() -> CloneFlags {
	CloneFlags::CLONE_NEWCGROUP |
		CloneFlags::CLONE_NEWIPC |
		CloneFlags::CLONE_NEWNET |
		CloneFlags::CLONE_NEWNS |
		CloneFlags::CLONE_NEWPID |
		CloneFlags::CLONE_NEWUSER |
		CloneFlags::CLONE_NEWUTS
}
