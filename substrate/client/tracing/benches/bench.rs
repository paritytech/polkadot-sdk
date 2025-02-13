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

use criterion::{criterion_group, criterion_main, Criterion};
use tracing_subscriber::fmt::{
	format,
	time::{ChronoLocal, FormatTime},
};

fn bench_fast_local_time(c: &mut Criterion) {
	c.bench_function("fast_local_time", |b| {
		let mut buffer = String::new();
		let t = sc_tracing::logging::FastLocalTime { with_fractional: true };
		b.iter(|| {
			buffer.clear();
			let mut writer = format::Writer::new(&mut buffer);
			t.format_time(&mut writer).unwrap();
		})
	});
}

// This is here just as a point of comparison.
fn bench_chrono_local(c: &mut Criterion) {
	c.bench_function("chrono_local", |b| {
		let mut buffer = String::new();
		let t = ChronoLocal::new("%Y-%m-%d %H:%M:%S%.3f".to_string());
		b.iter(|| {
			buffer.clear();
			let mut writer: format::Writer<'_> = format::Writer::new(&mut buffer);
			t.format_time(&mut writer).unwrap();
		})
	});
}

criterion_group! {
	name = benches;
	config = Criterion::default();
	targets = bench_fast_local_time, bench_chrono_local
}
criterion_main!(benches);
