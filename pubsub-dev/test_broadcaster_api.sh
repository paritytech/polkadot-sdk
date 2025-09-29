#!/bin/bash

# Enhanced test script for BroadcasterApi with human-readable output
# Tests with the exact data from your publish: ParaID 1000, key "0x123"

RPC_URL=${1:-"http://localhost:9900"}

# Function to decode hex to ASCII if possible
hex_to_ascii() {
    local hex=$1
    # Remove 0x prefix if present
    hex=${hex#0x}
    # Convert hex to ASCII, handling non-printable chars
    echo -n "$hex" | xxd -r -p 2>/dev/null | tr -d '\0' | cat -v
}

# Function to decode SCALE-encoded Option<Vec<u8>>
decode_option_vec() {
    local hex_result=$1
    # Remove 0x prefix
    hex_result=${hex_result#0x}

    if [[ -z "$hex_result" ]]; then
        echo "❌ None (empty result)"
        return
    fi

    # Check first byte for Option
    local option_byte=${hex_result:0:2}
    if [[ "$option_byte" == "00" ]]; then
        echo "❌ None"
        return
    elif [[ "$option_byte" == "01" ]]; then
        echo "✅ Some:"
        # Remove option byte
        hex_result=${hex_result:2}

        # Decode Vec<u8> - first get length
        local length_hex=${hex_result:0:2}
        local length=$((16#$length_hex))
        echo "   📏 Length: $length bytes"

        # Extract data bytes
        local data_hex=${hex_result:2:$((length*2))}
        echo "   🔢 Raw bytes: [$data_hex]"

        # Convert to decimal array
        local decimal_array=""
        for ((i=0; i<${#data_hex}; i+=2)); do
            local byte_hex=${data_hex:$i:2}
            local byte_dec=$((16#$byte_hex))
            decimal_array="$decimal_array$byte_dec, "
        done
        decimal_array=${decimal_array%, }
        echo "   🔢 Decimal: [$decimal_array]"

        # Try to decode as ASCII
        local ascii_text=$(hex_to_ascii "$data_hex")
        if [[ -n "$ascii_text" ]]; then
            echo "   📝 ASCII: \"$ascii_text\""
        fi
    else
        echo "❓ Unknown format: $hex_result"
    fi
}

echo "🔍 Testing BroadcasterApi with Enhanced Decoding..."
echo "   RPC URL: $RPC_URL"
echo "   Testing ParaID: 1000"
echo "   Testing Key: [48, 120, 49, 50, 51] (\"0x123\")"
echo ""

# SCALE-encoded parameters for get_published_value(1000, [48, 120, 49, 50, 51])
# ParaID 1000 = 0xe8030000 (little-endian u32)
# Vec<u8> [48, 120, 49, 50, 51] = 0x14307831323 (length prefix + bytes)
ENCODED_PARAMS="0xe8030000143078313233"

echo "📡 Calling BroadcasterApi_get_published_value..."
echo "   Parameters: $ENCODED_PARAMS"
echo ""

# Make the API call and extract result
RESPONSE=$(curl -s -H "Content-Type: application/json" -d "{
  \"id\": 1,
  \"jsonrpc\": \"2.0\",
  \"method\": \"state_call\",
  \"params\": [
    \"BroadcasterApi_get_published_value\",
    \"$ENCODED_PARAMS\"
  ]
}" $RPC_URL)

echo "📊 Raw Response:"
echo "$RESPONSE" | jq '.'
echo ""

# Extract and decode result
RESULT=$(echo "$RESPONSE" | jq -r '.result // empty')
if [[ -n "$RESULT" && "$RESULT" != "null" ]]; then
    echo "🔍 Decoded Result:"
    decode_option_vec "$RESULT"
else
    echo "❌ No result or error in response"
fi

echo ""
echo "=================================="
echo ""

# Also test get_publisher_child_root
echo "📡 Calling BroadcasterApi_get_publisher_child_root..."
PARA_ONLY="0xe8030000"  # Just ParaID 1000

ROOT_RESPONSE=$(curl -s -H "Content-Type: application/json" -d "{
  \"id\": 2,
  \"jsonrpc\": \"2.0\",
  \"method\": \"state_call\",
  \"params\": [
    \"BroadcasterApi_get_publisher_child_root\",
    \"$PARA_ONLY\"
  ]
}" $RPC_URL)

echo ""
echo "📊 Raw Response:"
echo "$ROOT_RESPONSE" | jq '.'
echo ""

# Extract and decode child root
ROOT_RESULT=$(echo "$ROOT_RESPONSE" | jq -r '.result // empty')
if [[ -n "$ROOT_RESULT" && "$ROOT_RESULT" != "null" ]]; then
    echo "🔍 Decoded Child Root:"
    if [[ "${ROOT_RESULT:0:4}" == "0x01" ]]; then
        echo "✅ Some:"
        ROOT_HASH=${ROOT_RESULT:4}
        echo "   🌳 Child Trie Root Hash: 0x$ROOT_HASH"
        echo "   📏 Hash Length: $((${#ROOT_HASH}/2)) bytes"
    else
        echo "❌ None (no published data)"
    fi
else
    echo "❌ No result or error in response"
fi

echo ""