// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
// Generated, do not edit!
// See ethereum client README.md for instructions to generate

use hex_literal::hex;
use snowbridge_beacon_primitives::{
	types::deneb, AncestryProof, BeaconHeader, ExecutionProof, VersionedExecutionPayloadHeader,
};
use snowbridge_outbound_queue_primitives::EventProof;
use snowbridge_verification_primitives::{EventFixture, Log, Proof};
use sp_core::U256;
use sp_std::vec;

pub fn make_submit_delivery_proof_message() -> EventFixture {
	EventFixture {
        event: EventProof {
            event_log: 	Log {
                address: hex!("b1185ede04202fe62d38f5db72f71e38ff3e8305").into(),
                topics: vec![
                    hex!("755d3b4d173427dc415f2c82a71641bfdbc1e8f79e36a2bd0d480237e94a159b").into(),
                    hex!("0000000000000000000000000000000000000000000000000000000000000000").into(),
                ],
                data: hex!("0000000000000000000000000000000000000000000000000000000000000001d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d").into(),
            },
            proof: Proof {
                receipt_proof: (vec![
                    hex!("9ffd15e48456a8bb393d6eb0a8f4183129ec4dc4a6ad7767f397e64c81decaa8").to_vec(),
                ], vec![
                    hex!("f9022e822080b9022802f9022401830d6c42b9010000000000000000000000000000000000080000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000002800000000000000000000000000000000000000001000000000000000020000000008000000000800000000020000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001000000000000000000000800000000000000000000000000000000000000004000000000000000000000000400000000000000000000000000000000020000000000200000000000000000000000000000000000000000000000000000000f90119f87a94b1185ede04202fe62d38f5db72f71e38ff3e8305f842a057f58171b8777633d03aff1e7408b96a3d910c93a7ce433a8cb7fb837dc306a6a09441dceeeffa7e032eedaccf9b7632e60e86711551a82ffbbb0dda8afd9e4ef7a0000000000000000000000000de45448ca2d57797c0bec0ee15a1e42334744219f89b94b1185ede04202fe62d38f5db72f71e38ff3e8305f842a0755d3b4d173427dc415f2c82a71641bfdbc1e8f79e36a2bd0d480237e94a159ba00000000000000000000000000000000000000000000000000000000000000000b8400000000000000000000000000000000000000000000000000000000000000001d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d").to_vec(),
                ]),
                execution_proof: ExecutionProof {
                    header: BeaconHeader {
                        slot: 430,
                        proposer_index: 4,
                        parent_root: hex!("1bd43f9a7f479d2b6faa4c79706085167c7b27a2af7d87bd2329d18d5199ef84").into(),
                        state_root: hex!("230a68261473d67eecf667a70ebe3a920f37b5612f7fa56c38cb28f25991d5ba").into(),
                        body_root: hex!("ee7c44894b7d8050b43f99313b63e082e1880fd86a0a1e8cf935ba0a210ed988").into(),
                    },
                        ancestry_proof: Some(AncestryProof {
                        header_branch: vec![
                            hex!("42bcf677791f0f162890b17bb11c91a6bc1c0fde2cb09e32c1d6bbbe13023eb6").into(),
                            hex!("1504a8e6c57d955d48f4104b8688f0c30126ca11bc82c1835a9ac8e925a98c85").into(),
                            hex!("85d10987d2bdd0846c6605474208d46bfc2891d01ca3a6467118326e3279f3c8").into(),
                            hex!("fadb936290b08a26f3ba2ed37a244370dd39fa9907ae5e5c810c3fe59a993e39").into(),
                            hex!("9e523139fb06f8edb7bc20f5a153f4eb63fa301f47141d1ed2b0305fcecbfd43").into(),
                            hex!("23c0d257e644ecc47bdb8c75865624d8744334711460a9a3586559c07776fa7d").into(),
                            hex!("702162a2d481b770dad7510f47e9ea98ac0b015a0df6cab1d1a2aa5f5d39134d").into(),
                            hex!("4bdf500cc3aaa125905585f58315108410591da16cec7e77db18182eb496e820").into(),
                            hex!("0537f753098eac69219eafd18de8fda23c1726cf48c858e8b1188a7b2d455a0e").into(),
                            hex!("527ff98a5fd464546baf5958a7840b3d918c9e1999b2e08c4b5ea4d68813eb07").into(),
                            hex!("14cebee5e9851ba12e5d2f0c981aa40161743fc089ff3fd35a61e1caadb01026").into(),
                            hex!("770461c335c58c69edfb854d1e00d9bcd7976a2c9d5ae72ed217286600193eb7").into(),
                            hex!("b7d05f875f140027ef5118a2247bbb84ce8f2f0f1123623085daf7960c329f5f").into(),
                        ],
                        finalized_block_root: hex!("34470b9a1b4fa4d8dfab4ff319179ec867260877882c80ca50c88eee2323bcd3").into(),
                        }),
                    execution_header: VersionedExecutionPayloadHeader::Deneb(deneb::ExecutionPayloadHeader {
                        parent_hash: hex!("c664880179a1498ffbb8ffdeaae8d3626dfd0f72469057a88cd3b09f833ab37a").into(),
                        fee_recipient: hex!("0000000000000000000000000000000000000000").into(),
                        state_root: hex!("268cd3d5fe59abe9fdec8aeb1165db91cb1f10e691f70e0a2a594403fe8ee8d7").into(),
                        receipts_root: hex!("9ffd15e48456a8bb393d6eb0a8f4183129ec4dc4a6ad7767f397e64c81decaa8").into(),
                        logs_bloom: hex!("00000000000000000000000000000000080000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000002800000000000000000000000000000000000000001000000000000000020000000008000000000800000000020000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001000000000000000000000800000000000000000000000000000000000000004000000000000000000000000400000000000000000000000000000000020000000000200000000000000000000000000000000000000000000000000000000").into(),
                        prev_randao: hex!("0863e00f4abf2f8c7b0be71f189e4ab49f10dce2b5b12e6b4ba1e6d59e0957d3").into(),
                        block_number: 430,
                        gas_limit: 52557587,
                        gas_used: 879682,
                        timestamp: 1742452214,
                        extra_data: hex!("d983010e0c846765746888676f312e32332e348664617277696e").into(),
                        base_fee_per_gas: U256::from(7u64),
                        block_hash: hex!("563d1588d66b823e7007681ce4d841830eb9c0bfd68bcff40c5b97b1e2900bfc").into(),
                        transactions_root: hex!("d67cb659bd89fcf2b568149a44cabad26ded416d986dc430967c614db5cb194b").into(),
                        withdrawals_root: hex!("792930bbd5baac43bcc798ee49aa8185ef76bb3b44ba62b91d86ae569e4bb535").into(),
                        blob_gas_used: 0,
                        excess_blob_gas: 0,
                    }),
                    execution_branch: vec![
                            hex!("4dd3ff42a7685e2028dc4bcb03ba0a9515a1c47fe7f3f904a5a46b3ae6105d73").into(),
                            hex!("b46f0c01805fe212e15907981b757e6c496b0cb06664224655613dcec82505bb").into(),
                            hex!("db56114e00fdd4c1f85c892bf35ac9a89289aaecb1ebd0a96cde606a748b5d71").into(),
                            hex!("13c659cf36c0fd6174fa4a9d4a0dfdb225c7e6df537591bf85f76cdc85825043").into(),
                    ],
                }
            },
        },
        finalized_header: BeaconHeader {
            slot: 2432,
            proposer_index: 7,
            parent_root: hex!("243c360605686dff7c26b870e8f8bf4b5228c2dad689ba8bebdd812562861ba2").into(),
            state_root: hex!("59da5ef8df70b8d7cc419da04f8bb4556ae57cb9b1be176e7cecdce6bcda3f0a").into(),
            body_root: hex!("250f60032b2262f5b782ca3bf8c418bea0f889b99b2e3f7b81ef41c5363c4c70").into(),
        },
        block_roots_root: hex!("0807920cbdbf5be8b77d67a6ad0563bebcf1b15eefbafa4827d9726f0f30e08e").into(),
    }
}
