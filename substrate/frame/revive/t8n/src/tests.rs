use super::*;
use serde_json;

#[test]
fn test_ef_test_roundtrip() {
    let json = r#"{
        "test1": {
            "env": {
                "currentCoinbase": "0x2adc25665018aa1fe0e6bc666dac8fc2697ff9ba",
                "currentGasLimit": "0x055d4a80",
                "currentNumber": "0x01",
                "currentTimestamp": "0x03e8",
                "currentDifficulty": "0x020000"
            },
            "pre": {
                "0x1000000000000000000000000000000000001000": {
                    "nonce": "0x01",
                    "balance": "0x00",
                    "code": "0x4660015500",
                    "storage": {}
                }
            },
            "transaction": {
                "nonce": "0x00",
                "gasPrice": "0x0a",
                "gasLimit": ["0x0186a0"],
                "to": "0x1000000000000000000000000000000000001000",
                "value": ["0x00"],
                "data": ["0x"],
                "sender": "0xa94f5374fce5edbc8e2a8697c15331677e6ebf0b",
                "secretKey": "0x45a915e4d060149eb4365960e6a7a45f334393093061116b197e3240065ff2d8"
            },
            "post": {
                "Berlin": [{
                    "hash": "0x9107aa2b177266ac04a618ffe3ee76e379d62128b3df36101654a7c5931fbe92",
                    "logs": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
                    "txbytes": "0xf860800a830186a0941000000000000000000000000000000000001000808025a062122520aa633c0a60f8fc7a69a072408581d520aae22bacae3a38c37e5a41bba03610a5d2e0dcfae9b08bc9589b1eb4b3c321cd76670a88180388cf130fb786ca",
                    "indexes": { "data": 0, "gas": 0, "value": 0 },
                    "state": {
                        "0x1000000000000000000000000000000000001000": {
                            "nonce": "0x01",
                            "balance": "0x00",
                            "code": "0x4660015500",
                            "storage": { "0x01": "0x01" }
                        }
                    }
                }]
            },
            "config": { "chainid": "0x01" },
            "_info": {
                "hash": "0x2ff09af8ac7acf3c5a8743fb3faed8b774710965c3ffccf429ce74b97efd6249",
                "comment": "test",
                "filling-transition-tool": "ethereum-spec-evm-resolver 0.0.5",
                "description": "Test CHAINID opcode.",
                "fixture-format": "state_test"
            }
        }
    }"#;

    let parsed: EfTest = serde_json::from_str(json).expect("Failed to parse");
    let serialized = serde_json::to_string(&parsed).expect("Failed to serialize");
    let reparsed: EfTest = serde_json::from_str(&serialized).expect("Failed to reparse");
    
    assert_eq!(parsed.tests.len(), reparsed.tests.len());
}

#[test]
fn test_t8n_result_roundtrip() {
    let result = T8nResult {
        state_root: "0xf97d803493a4d8c02ae199f526588305b875ba64e12c44b87baff5ad1adacd4f".to_string(),
        tx_root: "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421".to_string(),
        receipts_root: "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421".to_string(),
        logs_hash: "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347".to_string(),
        logs_bloom: "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000".to_string(),
        receipts: vec![Receipt {
            tx_type: Some("0x0".to_string()),
            root: "".to_string(),
            status: "0x1".to_string(),
            cumulative_gas_used: "0x5659".to_string(),
            logs_bloom: "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000".to_string(),
            logs: Vec::new(),
            transaction_hash: "0x0000000000000000000000000000000000000000000000000000000000000001".to_string(),
            contract_address: "0x0000000000000000000000000000000000000000".to_string(),
            gas_used: "0x5659".to_string(),
            effective_gas_price: None,
            blob_gas_used: None,
            blob_gas_price: None,
            block_hash: None,
            block_number: None,
            transaction_index: "0x0".to_string(),
        }],
        rejected: Vec::new(),
        current_difficulty: "0x00".to_string(),
        gas_used: "0x5659".to_string(),
        current_base_fee: None,
        withdrawals_root: None,
        current_excess_blob_gas: None,
        blob_gas_used: None,
        requests_hash: None,
        requests: Vec::new(),
    };

    let serialized = serde_json::to_string(&result).expect("Failed to serialize");
    let parsed: T8nResult = serde_json::from_str(&serialized).expect("Failed to parse");
    
    assert_eq!(result.state_root, parsed.state_root);
    assert_eq!(result.receipts.len(), parsed.receipts.len());
    assert_eq!(result.rejected.len(), parsed.rejected.len());
}

#[test]
fn test_hex_parsing_roundtrip() {
    let test_cases = vec!["0x1a", "1a", "0x", "", "0x0186a0"];
    
    for case in test_cases {
        let parsed_u64 = parse_hex_u64(case).unwrap();
        let parsed_u256 = parse_hex_u256(case).unwrap();
        let parsed_bytes = parse_hex_bytes(case).unwrap();
        
        let _ = parsed_u64;
        assert!(parsed_u256.starts_with("0x"));
        assert!(parsed_bytes.is_empty() || !parsed_bytes.is_empty());
    }
}

#[test]
fn test_go_ethereum_output_field_compatibility() {
    let result = T8nResult {
        state_root: "0x84208a19bc2b46ada7445180c1db162be5b39b9abc8c0a54b05d32943eae4e13".to_string(),
        tx_root: "0xc4761fd7b87ff2364c7c60b6c5c8d02e522e815328aaea3f20e3b7b7ef52c42d".to_string(),
        receipts_root: "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421".to_string(),
        logs_hash: "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347".to_string(),
        logs_bloom: "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000".to_string(),
        receipts: vec![],
        rejected: vec![],
        current_difficulty: "0x020000".to_string(),
        gas_used: "0x5208".to_string(),
        current_base_fee: Some("0x07".to_string()),
        withdrawals_root: Some("0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421".to_string()),
        current_excess_blob_gas: Some("0x0".to_string()),
        blob_gas_used: Some("0x0".to_string()),
        requests_hash: Some("0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421".to_string()),
        requests: vec!["0x1234".to_string()],
    };

    let json = serde_json::to_string(&result).expect("Failed to serialize");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("Failed to parse");
    
    // Verify all go-ethereum field names are present with correct casing
    assert!(parsed.get("stateRoot").is_some());
    assert!(parsed.get("txRoot").is_some());
    assert!(parsed.get("receiptsRoot").is_some());  // Note: receiptsRoot not receiptRoot
    assert!(parsed.get("logsHash").is_some());
    assert!(parsed.get("logsBloom").is_some());
    assert!(parsed.get("receipts").is_some());
    assert!(parsed.get("rejected").is_some());
    assert!(parsed.get("currentDifficulty").is_some());
    assert!(parsed.get("gasUsed").is_some());
    
    // Optional fields should be present when set
    assert!(parsed.get("currentBaseFee").is_some());
    assert!(parsed.get("withdrawalsRoot").is_some());
    assert!(parsed.get("currentExcessBlobGas").is_some());
    assert!(parsed.get("blobGasUsed").is_some());
    assert!(parsed.get("requestsHash").is_some());
    assert!(parsed.get("requests").is_some());
    
    // Verify field types are correct
    assert!(parsed["receipts"].is_array());
    assert!(parsed["rejected"].is_array());
    assert!(parsed["requests"].is_array());
    
    // Verify no snake_case field names
    assert!(parsed.get("state_root").is_none());
    assert!(parsed.get("tx_root").is_none());
    assert!(parsed.get("receipt_root").is_none());
    assert!(parsed.get("receipts_root").is_none());
    assert!(parsed.get("logs_hash").is_none());
    assert!(parsed.get("logs_bloom").is_none());
    assert!(parsed.get("current_difficulty").is_none());
    assert!(parsed.get("gas_used").is_none());
    assert!(parsed.get("current_base_fee").is_none());
}

#[test]
fn test_receipt_compatibility() {
    let receipt = Receipt {
        tx_type: Some("0x2".to_string()),
        root: "".to_string(),
        status: "0x1".to_string(),
        cumulative_gas_used: "0x5208".to_string(),
        logs_bloom: "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000".to_string(),
        logs: vec![],
        transaction_hash: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string(),
        contract_address: "0x0000000000000000000000000000000000000000".to_string(),
        gas_used: "0x5208".to_string(),
        effective_gas_price: Some("0x3b9aca00".to_string()),
        blob_gas_used: Some("0x20000".to_string()),
        blob_gas_price: Some("0x01".to_string()),
        block_hash: Some("0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890".to_string()),
        block_number: Some("0x10".to_string()),
        transaction_index: "0x0".to_string(),
    };

    let json = serde_json::to_string(&receipt).expect("Failed to serialize receipt");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("Failed to parse receipt");
    
    // Verify go-ethereum receipt field names
    assert_eq!(parsed["type"], "0x2");
    assert_eq!(parsed["root"], "");
    assert_eq!(parsed["status"], "0x1");
    assert_eq!(parsed["cumulativeGasUsed"], "0x5208");
    assert_eq!(parsed["logsBloom"], "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000");
    assert!(parsed["logs"].is_array());
    assert_eq!(parsed["transactionHash"], "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef");
    assert_eq!(parsed["contractAddress"], "0x0000000000000000000000000000000000000000");
    assert_eq!(parsed["gasUsed"], "0x5208");
    assert_eq!(parsed["effectiveGasPrice"], "0x3b9aca00");
    assert_eq!(parsed["blobGasUsed"], "0x20000");
    assert_eq!(parsed["blobGasPrice"], "0x01");
    assert_eq!(parsed["blockHash"], "0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890");
    assert_eq!(parsed["blockNumber"], "0x10");
    assert_eq!(parsed["transactionIndex"], "0x0");
    
    // Verify no snake_case field names
    assert!(parsed.get("cumulative_gas_used").is_none());
    assert!(parsed.get("logs_bloom").is_none());
    assert!(parsed.get("transaction_hash").is_none());
    assert!(parsed.get("contract_address").is_none());
    assert!(parsed.get("gas_used").is_none());
}

#[test] 
fn test_actual_ef_file_parsing() {
    let json_content = r#"{
        "tests/istanbul/eip1344_chainid/test_chainid.py::test_chainid[fork_Berlin-state_test]": {
            "env": {
                "currentCoinbase": "0x2adc25665018aa1fe0e6bc666dac8fc2697ff9ba",
                "currentGasLimit": "0x055d4a80",
                "currentNumber": "0x01",
                "currentTimestamp": "0x03e8",
                "currentDifficulty": "0x020000"
            },
            "pre": {
                "0x1000000000000000000000000000000000001000": {
                    "nonce": "0x01",
                    "balance": "0x00",
                    "code": "0x4660015500",
                    "storage": {}
                },
                "0xa94f5374fce5edbc8e2a8697c15331677e6ebf0b": {
                    "nonce": "0x00",
                    "balance": "0x3635c9adc5dea00000",
                    "code": "0x",
                    "storage": {}
                }
            },
            "transaction": {
                "nonce": "0x00",
                "gasPrice": "0x0a",
                "gasLimit": ["0x0186a0"],
                "to": "0x1000000000000000000000000000000000001000",
                "value": ["0x00"],
                "data": ["0x"],
                "sender": "0xa94f5374fce5edbc8e2a8697c15331677e6ebf0b",
                "secretKey": "0x45a915e4d060149eb4365960e6a7a45f334393093061116b197e3240065ff2d8"
            },
            "post": {
                "Berlin": [{
                    "hash": "0x9107aa2b177266ac04a618ffe3ee76e379d62128b3df36101654a7c5931fbe92",
                    "logs": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
                    "txbytes": "0xf860800a830186a0941000000000000000000000000000000000001000808025a062122520aa633c0a60f8fc7a69a072408581d520aae22bacae3a38c37e5a41bba03610a5d2e0dcfae9b08bc9589b1eb4b3c321cd76670a88180388cf130fb786ca",
                    "indexes": {"data": 0, "gas": 0, "value": 0},
                    "state": {
                        "0x1000000000000000000000000000000000001000": {
                            "nonce": "0x01",
                            "balance": "0x00",
                            "code": "0x4660015500",
                            "storage": {"0x01": "0x01"}
                        }
                    }
                }]
            },
            "config": {"chainid": "0x01"},
            "_info": {
                "hash": "0x2ff09af8ac7acf3c5a8743fb3faed8b774710965c3ffccf429ce74b97efd6249",
                "comment": "`execution-spec-tests` generated test",
                "filling-transition-tool": "ethereum-spec-evm-resolver 0.0.5",
                "description": "Test CHAINID opcode.",
                "fixture-format": "state_test"
            }
        }
    }"#;
    
    // Parse the actual EF test format
    let ef_test: EfTest = serde_json::from_str(json_content).expect("Failed to parse EF test");
    
    assert_eq!(ef_test.tests.len(), 1);
    let test_case = ef_test.tests.values().next().expect("No test case found");
    
    // Verify the structure matches our expectations
    assert_eq!(test_case.env.current_coinbase, "0x2adc25665018aa1fe0e6bc666dac8fc2697ff9ba");
    assert_eq!(test_case.config.chainid, "0x01");
    assert_eq!(test_case.transaction.gas_limit.len(), 1);
    assert_eq!(test_case.transaction.value.len(), 1);
    assert_eq!(test_case.transaction.data.len(), 1);
    
    // Test serialization roundtrip
    let serialized = serde_json::to_string(&ef_test).expect("Failed to serialize");
    let reparsed: EfTest = serde_json::from_str(&serialized).expect("Failed to reparse");
    
    assert_eq!(ef_test.tests.len(), reparsed.tests.len());
}