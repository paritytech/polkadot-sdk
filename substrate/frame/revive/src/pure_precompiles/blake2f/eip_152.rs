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

/// The precomputed values for BLAKE2b [from the spec](https://tools.ietf.org/html/rfc7693#section-2.7)
/// There are 10 16-byte arrays - one for each round
/// the entries are calculated from the sigma constants.
const SIGMA: [[usize; 16]; 10] = [
	[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
	[14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
	[11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
	[7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
	[9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
	[2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
	[12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
	[13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
	[6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
	[10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
];

/// IV is the initialization vector for BLAKE2b. See https://tools.ietf.org/html/rfc7693#section-2.6
/// for details.
const IV: [u64; 8] = [
	0x6a09e667f3bcc908,
	0xbb67ae8584caa73b,
	0x3c6ef372fe94f82b,
	0xa54ff53a5f1d36f1,
	0x510e527fade682d1,
	0x9b05688c2b3e6c1f,
	0x1f83d9abfb41bd6b,
	0x5be0cd19137e2179,
];

#[inline(always)]
/// The G mixing function. See https://tools.ietf.org/html/rfc7693#section-3.1
fn g(v: &mut [u64], a: usize, b: usize, c: usize, d: usize, x: u64, y: u64) {
	v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
	v[d] = (v[d] ^ v[a]).rotate_right(32);
	v[c] = v[c].wrapping_add(v[d]);
	v[b] = (v[b] ^ v[c]).rotate_right(24);
	v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
	v[d] = (v[d] ^ v[a]).rotate_right(16);
	v[c] = v[c].wrapping_add(v[d]);
	v[b] = (v[b] ^ v[c]).rotate_right(63);
}

/// The Blake2 compression function F. See https://tools.ietf.org/html/rfc7693#section-3.2
/// Takes as an argument the state vector `h`, message block vector `m`, offset counter `t`, final
/// block indicator flag `f`, and number of rounds `rounds`. The state vector provided as the first
/// parameter is modified by the function.
pub fn compress(h: &mut [u64; 8], m: [u64; 16], t: [u64; 2], f: bool, rounds: usize) {
	let mut v = [0u64; 16];
	v[..h.len()].copy_from_slice(h); // First half from state.
	v[h.len()..].copy_from_slice(&IV); // Second half from IV.

	v[12] ^= t[0];
	v[13] ^= t[1];

	if f {
		v[14] = !v[14] // Invert all bits if the last-block-flag is set.
	}
	for i in 0..rounds {
		// Message word selection permutation for this round.
		let s = &SIGMA[i % 10];
		g(&mut v, 0, 4, 8, 12, m[s[0]], m[s[1]]);
		g(&mut v, 1, 5, 9, 13, m[s[2]], m[s[3]]);
		g(&mut v, 2, 6, 10, 14, m[s[4]], m[s[5]]);
		g(&mut v, 3, 7, 11, 15, m[s[6]], m[s[7]]);

		g(&mut v, 0, 5, 10, 15, m[s[8]], m[s[9]]);
		g(&mut v, 1, 6, 11, 12, m[s[10]], m[s[11]]);
		g(&mut v, 2, 7, 8, 13, m[s[12]], m[s[13]]);
		g(&mut v, 3, 4, 9, 14, m[s[14]], m[s[15]]);
	}

	for i in 0..8 {
		h[i] ^= v[i] ^ v[i + 8];
	}
}
