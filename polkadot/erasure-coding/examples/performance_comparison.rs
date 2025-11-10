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

//! Performance comparison between erasure coding v1 and v2

use polkadot_erasure_coding::{obtain_chunks_v1, reconstruct_v1, reconstruct_from_systematic_v1, systematic_recovery_threshold};
use polkadot_erasure_coding::v2::{obtain_chunks_v2, reconstruct_v2, reconstruct_from_systematic_v2};
use polkadot_node_primitives::{AvailableData, BlockData, PoV};
use polkadot_primitives::PersistedValidationData;
use std::sync::Arc;
use std::time::{Duration, Instant};
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

const KB: usize = 1024;
const MB: usize = 1024 * KB;

// Test configurations: data sizes and chunk counts
const DATA_SIZES: &[(usize, &str)] = &[
    (1 * KB, "1 KB"),
    (500 * KB, "500 KB"),
    (1 * MB, "1 MB"),
    (5 * MB, "5 MB"),
    (10 * MB, "10 MB"),
];

const CHUNK_COUNTS: &[usize] = &[5, 10, 100, 1000];

const ITERATIONS: usize = 10;

#[derive(Debug)]
struct BenchmarkResult {
    data_size: String,
    n_chunks: usize,
    v1_encode_avg: Duration,
    v2_encode_avg: Duration,
    v1_decode_avg: Duration,
    v2_decode_avg: Duration,
    v1_systematic_decode_avg: Duration,
    v2_systematic_decode_avg: Duration,
}

fn create_test_data(size: usize, seed: u64) -> AvailableData {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut data = vec![0u8; size];
    rng.fill(&mut data[..]);
    
    let pov = PoV {
        block_data: BlockData(data),
    };
    
    AvailableData {
        pov: Arc::new(pov),
        validation_data: PersistedValidationData::default(),
    }
}

fn benchmark_encode_v1(n_validators: usize, data: &AvailableData) -> (Duration, Vec<Vec<u8>>) {
    let start = Instant::now();
    let chunks = obtain_chunks_v1(n_validators, data).unwrap();
    let duration = start.elapsed();
    (duration, chunks)
}

fn benchmark_encode_v2(n_validators: usize, data: &AvailableData) -> (Duration, Vec<Vec<u8>>) {
    let start = Instant::now();
    let chunks = obtain_chunks_v2(n_validators, data).unwrap();
    let duration = start.elapsed();
    (duration, chunks)
}

fn benchmark_decode_v1(n_validators: usize, chunks: &[Vec<u8>]) -> (Duration, AvailableData) {
    // Use minimum required chunks for reconstruction
    let threshold = polkadot_erasure_coding::recovery_threshold(n_validators).unwrap();
    let chunks_for_reconstruction: Vec<_> = chunks
        .iter()
        .enumerate()
        .take(threshold)
        .map(|(i, c)| (&c[..], i))
        .collect();
    
    let start = Instant::now();
    let reconstructed: AvailableData = reconstruct_v1(n_validators, chunks_for_reconstruction).unwrap();
    let duration = start.elapsed();
    
    (duration, reconstructed)
}

fn benchmark_decode_v2(n_validators: usize, chunks: &[Vec<u8>]) -> (Duration, AvailableData) {
    // Use minimum required chunks for reconstruction
    let threshold = polkadot_erasure_coding::recovery_threshold(n_validators).unwrap();
    let chunks_for_reconstruction: Vec<_> = chunks
        .iter()
        .enumerate()
        .take(threshold)
        .map(|(i, c)| (&c[..], i))
        .collect();
    
    let start = Instant::now();
    let reconstructed: AvailableData = reconstruct_v2(n_validators, chunks_for_reconstruction).unwrap();
    let duration = start.elapsed();
    
    (duration, reconstructed)
}

fn benchmark_systematic_decode_v1(n_validators: usize, chunks: &[Vec<u8>]) -> Option<(Duration, AvailableData)> {
    // Use systematic chunks (first k chunks in order)
    let k = systematic_recovery_threshold(n_validators).unwrap();
    
    // Check if we have enough chunks
    if k > chunks.len() {
        return None;
    }
    
    let systematic_chunks: Vec<Vec<u8>> = chunks
        .iter()
        .take(k)
        .cloned()
        .collect();
    
    let start = Instant::now();
    let reconstructed: AvailableData = reconstruct_from_systematic_v1(n_validators, systematic_chunks).unwrap();
    let duration = start.elapsed();
    
    Some((duration, reconstructed))
}

fn benchmark_systematic_decode_v2(n_validators: usize, chunks: &[Vec<u8>]) -> Option<(Duration, AvailableData)> {
    let systematic_chunks: Vec<Vec<u8>> = chunks
        .iter()
        .cloned()
        .collect();
    
    let start = Instant::now();
    let reconstructed: AvailableData = reconstruct_from_systematic_v2(n_validators, systematic_chunks).unwrap();
    let duration = start.elapsed();
    
    Some((duration, reconstructed))
}

fn run_benchmark(data_size: usize, data_size_name: &str, n_chunks: usize) -> BenchmarkResult {
    println!("  Running: {} data, {} chunks...", data_size_name, n_chunks);
    
    let mut v1_encode_times = Vec::new();
    let mut v2_encode_times = Vec::new();
    let mut v1_decode_times = Vec::new();
    let mut v2_decode_times = Vec::new();
    let mut v1_systematic_decode_times = Vec::new();
    let mut v2_systematic_decode_times = Vec::new();
    
    for iteration in 0..ITERATIONS {
        // Create random test data with unique seed for each iteration
        let data = create_test_data(data_size, iteration as u64);
        
        // Benchmark v1 encode
        let (v1_encode_time, v1_chunks) = benchmark_encode_v1(n_chunks, &data);
        v1_encode_times.push(v1_encode_time);
        
        // Benchmark v2 encode
        let (v2_encode_time, v2_chunks) = benchmark_encode_v2(n_chunks, &data);
        v2_encode_times.push(v2_encode_time);
        
        // Benchmark v1 decode
        let (v1_decode_time, v1_reconstructed) = benchmark_decode_v1(n_chunks, &v1_chunks);
        v1_decode_times.push(v1_decode_time);
        
        // Verify v1 reconstruction (outside of timing measurement)
        assert_eq!(
            v1_reconstructed, data,
            "V1 reconstruction mismatch for {} data, {} chunks, iteration {}",
            data_size_name, n_chunks, iteration
        );
        
        // Benchmark v2 decode
        let (v2_decode_time, v2_reconstructed) = benchmark_decode_v2(n_chunks, &v2_chunks);
        v2_decode_times.push(v2_decode_time);
        
        // Verify v2 reconstruction (outside of timing measurement)
        assert_eq!(
            v2_reconstructed, data,
            "V2 reconstruction mismatch for {} data, {} chunks, iteration {}",
            data_size_name, n_chunks, iteration
        );
        
        // Benchmark v1 systematic decode
        if let Some((v1_systematic_decode_time, v1_systematic_reconstructed)) = benchmark_systematic_decode_v1(n_chunks, &v1_chunks) {
            v1_systematic_decode_times.push(v1_systematic_decode_time);
            
            // Verify v1 systematic reconstruction (outside of timing measurement)
            assert_eq!(
                v1_systematic_reconstructed, data,
                "V1 systematic reconstruction mismatch for {} data, {} chunks, iteration {}",
                data_size_name, n_chunks, iteration
            );
        }
        
        // Benchmark v2 systematic decode
        if let Some((v2_systematic_decode_time, v2_systematic_reconstructed)) = benchmark_systematic_decode_v2(n_chunks, &v2_chunks) {
            v2_systematic_decode_times.push(v2_systematic_decode_time);
            
            // Verify v2 systematic reconstruction (outside of timing measurement)
            assert_eq!(
                v2_systematic_reconstructed, data,
                "V2 systematic reconstruction mismatch for {} data, {} chunks, iteration {}",
                data_size_name, n_chunks, iteration
            );
        }
    }
    
    // Calculate averages
    let v1_encode_avg = v1_encode_times.iter().sum::<Duration>() / ITERATIONS as u32;
    let v2_encode_avg = v2_encode_times.iter().sum::<Duration>() / ITERATIONS as u32;
    let v1_decode_avg = v1_decode_times.iter().sum::<Duration>() / ITERATIONS as u32;
    let v2_decode_avg = v2_decode_times.iter().sum::<Duration>() / ITERATIONS as u32;
    
    // For systematic reconstruction, use average of available measurements (or zero if none)
    let v1_systematic_decode_avg = if !v1_systematic_decode_times.is_empty() {
        v1_systematic_decode_times.iter().sum::<Duration>() / v1_systematic_decode_times.len() as u32
    } else {
        Duration::ZERO
    };
    let v2_systematic_decode_avg = if !v2_systematic_decode_times.is_empty() {
        v2_systematic_decode_times.iter().sum::<Duration>() / v2_systematic_decode_times.len() as u32
    } else {
        Duration::ZERO
    };
    
    BenchmarkResult {
        data_size: data_size_name.to_string(),
        n_chunks,
        v1_encode_avg,
        v2_encode_avg,
        v1_decode_avg,
        v2_decode_avg,
        v1_systematic_decode_avg,
        v2_systematic_decode_avg,
    }
}

fn format_duration(d: Duration) -> String {
    if d.is_zero() {
        return "N/A".to_string();
    }
    
    let micros = d.as_micros();
    if micros < 1000 {
        format!("{} μs", micros)
    } else if micros < 1_000_000 {
        format!("{:.2} ms", micros as f64 / 1000.0)
    } else {
        format!("{:.2} s", micros as f64 / 1_000_000.0)
    }
}

fn format_speedup(v1: Duration, v2: Duration) -> String {
    if v1.is_zero() || v2.is_zero() {
        return "N/A".to_string();
    }
    
    let ratio = v1.as_micros() as f64 / v2.as_micros() as f64;
    format!("{:.2}x", ratio)
}

fn print_results(results: &[BenchmarkResult]) {
    // Print encoding results
    println!("\n╔════════════════════════════════════════════════════════════════════════════════════════════════════════╗");
    println!("║                             ERASURE CODING V1 vs V2 - ENCODING                                         ║");
    println!("╠═══════════╦═════════╦════════════╦════════════╦════════════════════════════════════════════════════════╣");
    println!("║   Size    ║ Chunks  ║ V1 Encode  ║ V2 Encode  ║  Speedup                                               ║");
    println!("╠═══════════╬═════════╬════════════╬════════════╬════════════════════════════════════════════════════════╣");
    
    for result in results {
        println!(
            "║ {:>9} ║ {:>7} ║ {:>10} ║ {:>10} ║ {:>10}                                             ║",
            result.data_size,
            result.n_chunks,
            format_duration(result.v1_encode_avg),
            format_duration(result.v2_encode_avg),
            format_speedup(result.v1_encode_avg, result.v2_encode_avg),
        );
    }
    
    println!("╚═══════════╩═════════╩════════════╩════════════╩════════════════════════════════════════════════════════╝");
    
    // Print regular decoding results
    println!("\n╔════════════════════════════════════════════════════════════════════════════════════════════════════════╗");
    println!("║                          ERASURE CODING V1 vs V2 - REGULAR RECONSTRUCTION                              ║");
    println!("╠═══════════╦═════════╦════════════╦════════════╦════════════════════════════════════════════════════════╣");
    println!("║   Size    ║ Chunks  ║ V1 Decode  ║ V2 Decode  ║  Speedup                                               ║");
    println!("╠═══════════╬═════════╬════════════╬════════════╬════════════════════════════════════════════════════════╣");
    
    for result in results {
        println!(
            "║ {:>9} ║ {:>7} ║ {:>10} ║ {:>10} ║ {:>10}                                             ║",
            result.data_size,
            result.n_chunks,
            format_duration(result.v1_decode_avg),
            format_duration(result.v2_decode_avg),
            format_speedup(result.v1_decode_avg, result.v2_decode_avg),
        );
    }
    
    println!("╚═══════════╩═════════╩════════════╩════════════╩════════════════════════════════════════════════════════╝");
    
    // Print systematic decoding results
    println!("\n╔════════════════════════════════════════════════════════════════════════════════════════════════════════╗");
    println!("║                        ERASURE CODING V1 vs V2 - SYSTEMATIC RECONSTRUCTION                             ║");
    println!("╠═══════════╦═════════╦════════════╦════════════╦════════════════════════════════════════════════════════╣");
    println!("║   Size    ║ Chunks  ║  V1 Syst   ║  V2 Syst   ║  Speedup                                               ║");
    println!("╠═══════════╬═════════╬════════════╬════════════╬════════════════════════════════════════════════════════╣");
    
    for result in results {
        println!(
            "║ {:>9} ║ {:>7} ║ {:>10} ║ {:>10} ║ {:>10}                                             ║",
            result.data_size,
            result.n_chunks,
            format_duration(result.v1_systematic_decode_avg),
            format_duration(result.v2_systematic_decode_avg),
            format_speedup(result.v1_systematic_decode_avg, result.v2_systematic_decode_avg),
        );
    }
    
    println!("╚═══════════╩═════════╩════════════╩════════════╩════════════════════════════════════════════════════════╝");
}

fn main() {
    println!("Running erasure coding v1 vs v2 performance comparison benchmark");
    println!("Number of iterations per test: {}", ITERATIONS);
    println!();
    
    let mut all_results = Vec::new();
    
    for &(data_size, data_size_name) in DATA_SIZES {
        for &n_chunks in CHUNK_COUNTS {
            println!("Testing: {} data, {} chunks", data_size_name, n_chunks);
            let result = run_benchmark(data_size, data_size_name, n_chunks);
            all_results.push(result);
        }
    }
    
    print_results(&all_results);
}

