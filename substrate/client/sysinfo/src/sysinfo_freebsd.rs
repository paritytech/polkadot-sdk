// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use sc_telemetry::SysInfo;
use std::{process::Command, str};

fn sysctl_output(name: &str) -> Option<String> {
	let output = Command::new("sysctl").arg("-n").arg(name).output().ok()?;
	let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
	Some(stdout)
}

pub fn gather_freebsd_sysinfo(sysinfo: &mut SysInfo) {
	if let Some(cpu_model) = sysctl_output("hw.model") {
		sysinfo.cpu = Some(cpu_model);
	}

	if let Some(cores) = sysctl_output("kern.smp.cores") {
		sysinfo.core_count = cores.parse().ok();
	}

	if let Some(memory) = sysctl_output("hw.physmem") {
		sysinfo.memory = memory.parse().ok();
	}

	if let Some(virtualization) = sysctl_output("kern.vm_guest") {
		sysinfo.is_virtual_machine = Some(virtualization != "none");
	}

	if let Some(freebsd_version) = sysctl_output("kern.osrelease") {
		sysinfo.linux_distro = Some(freebsd_version);
	}
}
