// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.
//
// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Example demonstrating the usage of v2 erasure coding functions.
//! 
//! This example shows how to use the new v2 functions that leverage
//! the external erasure-coding package for improved performance.

use polkadot_erasure_coding::v2::{obtain_chunks_v2, reconstruct_v2, reconstruct_from_systematic_v2};
use polkadot_erasure_coding::v2::fast::{fast_encode, fast_decode};
use polkadot_node_primitives::{AvailableData, BlockData, PoV};
use polkadot_primitives::{HeadData, PersistedValidationData};
use std::sync::Arc;

fn main() {
    println!("=== Polkadot Erasure Coding v2 Example ===");
    
    // Create sample data
    let pov = PoV { 
        block_data: BlockData((0..255).collect()) 
    };
    let available_data = AvailableData { 
        pov: Arc::new(pov), 
        validation_data: PersistedValidationData {
            parent_head: HeadData(vec![1, 2, 3, 4, 5]),
            relay_parent_number: 12345,
            relay_parent_storage_root: [0u8; 32].into(),
            max_pov_size: 5 * 1024 * 1024,
        }
    };
    
    let n_validators = 10;
    
    println!("Creating erasure-coded chunks for {} validators...", n_validators);
    
    // Test v2 functions
    match obtain_chunks_v2(n_validators, &available_data) {
        Ok(chunks) => {
            println!("✓ Successfully created {} chunks", chunks.len());
            
            // Test systematic reconstruction
            println!("Testing systematic reconstruction...");
            match reconstruct_from_systematic_v2(n_validators, chunks.clone()) {
                Ok(reconstructed) => {
                    println!("✓ Systematic reconstruction successful");
                    assert_eq!(reconstructed, available_data);
                }
                Err(e) => println!("✗ Systematic reconstruction failed: {:?}", e),
            }
            
            // Test general reconstruction with specific chunks
            println!("Testing general reconstruction...");
            let test_chunks = [(&*chunks[1], 1), (&*chunks[4], 4), (&*chunks[6], 6), (&*chunks[9], 9)];
            match reconstruct_v2(n_validators, test_chunks.iter().cloned()) {
                Ok(reconstructed) => {
                    println!("✓ General reconstruction successful");
                    assert_eq!(reconstructed, available_data);
                }
                Err(e) => println!("✗ General reconstruction failed: {:?}", e),
            }
        }
        Err(e) => println!("✗ Failed to create chunks: {:?}", e),
    }
    
    // Test fast functions
    println!("\nTesting fast encoding/decoding...");
    match fast_encode(n_validators, &available_data) {
        Ok(chunks) => {
            println!("✓ Fast encoding successful");
            
            let test_chunks = [(&*chunks[1], 1), (&*chunks[4], 4), (&*chunks[6], 6), (&*chunks[9], 9)];
            match fast_decode::<_, AvailableData>(n_validators, test_chunks.iter().cloned()) {
                Ok(reconstructed) => {
                    println!("✓ Fast decoding successful");
                    assert_eq!(reconstructed, available_data);
                }
                Err(e) => println!("✗ Fast decoding failed: {:?}", e),
            }
        }
        Err(e) => println!("✗ Fast encoding failed: {:?}", e),
    }
    
    println!("\n=== Example completed successfully! ===");
}
