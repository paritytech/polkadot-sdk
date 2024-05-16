#!/usr/bin/env python3
import re
import sys
import os

def convert_to_microseconds(value, unit):
    if unit == 'µs':
        return float(value)
    elif unit == 'ms':
        return float(value) * 1000
    elif unit == 's':
        return float(value) * 1000000
    else:
        raise ValueError("Unit not recognized")

def ensure_dir_exists(dir_name):
    """Ensure that a directory exists in the current directory. If it doesn't, create it."""
    path = os.path.join(os.getcwd(), dir_name)
    if not os.path.exists(path):
        os.makedirs(path)
        print(f"Directory '{dir_name}' was created.")
    else:
        print(f"Directory '{dir_name}' already exists.")

def parse_line(line, patterns):
    for pattern in patterns:
        if pattern["guard"] in line:
            match = re.match(pattern["regex"], line)
            if match:
                return pattern["type"], pattern["extract_data"](match)

    return None

def parse_log_file(patterns, filepath):

    parsed_data = {pattern['type']: {'data': [], 'columns': pattern['column_names']} for pattern in patterns}

    try:
        with open(filepath, 'r') as file:
            for line in file:
                parsed = parse_line(line, patterns)
                if parsed:
                    parsed_data[parsed[0]]['data'].append(parsed[1])
    except FileNotFoundError:
        print(f"Error: The file {filepath} does not exist.")
    except Exception as e:
        print(f"An error occurred: {e}")

    return parsed_data

def save_parsed_data(data, columns, output_filepath):
    with open(output_filepath, 'w') as file:
        file.write("\t".join(columns) + "\n")
        for entry in data:
            file.write("\t".join(map(str, entry)) + "\n")

def main():
    if len(sys.argv) < 3:
        print("Usage: python script_name.py <path_to_log_file> <output_dir>")
        sys.exit(1)

    patterns = [
        {
            "type": "validate_transaction",
            "regex": r'(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2}\.\d{3}) DEBUG.*\[([0-9a-fx]+)\].*runtime_api.validate_transaction: (\d+\.\d+)(µs|ms)',
            "guard": "runtime_api.validate_transaction",
            "column_names": ["date", "time", "transaction_id", "duration"],
            "extract_data": lambda match: (
                    match.group(1),
                    match.group(2),
                    match.group(3),
                    convert_to_microseconds(match.group(4), match.group(5))
                )
        },
        {
            "type": "import",
            "regex": r'(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2}\.\d{3})  INFO.*✨ Imported #(\d+) \((0x[a-f0-9…]+)\)',
            "guard": "✨ Imported",
            "column_names": ["date", "time", "block_number", "block_hash"],
            "extract_data": lambda match: (
                    match.group(1),
                    match.group(2),
                    match.group(3),
                    match.group(4)
                )
        },
        {
            "type": "txpool_maintain",
            "regex": "(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2}\.\d{3})  INFO.*txpool: maintain: txs:\((\d+), (\d+)\) views:\[(\d+);.*\] event:(NewBestBlock|Finalized) {.*}  took:(\d+\.\d+)([µms]+)",
            "guard": "txpool: maintain:",
            "column_names": ["date", "time", "unwatched_txs", "watched_txs", "views_count", "event", "duration"],
            "extract_data": lambda match: (
                    match.group(1),
                    match.group(2),
                    match.group(3),
                    match.group(4),
                    match.group(5),
                    1 if match.group(6) == "NewBestBlock" else 2,
                    convert_to_microseconds(match.group(7), match.group(8))
                )
        },
        {
            "type": "propagate_transaction", 
            "regex": r'(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2}\.\d{3}) DEBUG.*Propagating transaction \[.*\]',
            "guard": "Propagating transaction [",
            "column_names": ["date", "time", "value"],
            "extract_data": lambda match: (
                    match.group(1),
                    match.group(2),
                    1
                )
        },
        {
            "type": "import_transaction", 
            "regex": r'(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2}\.\d{3}) DEBUG.*import transaction: (\d+\.\d+)(µs|ms)',
            "guard": "import transaction",
            "column_names": ["date", "time", "duration"],
            "extract_data": lambda match: (
                match.group(1),
                match.group(2),
                convert_to_microseconds(match.group(3), match.group(4))
                )
        },
        {
            "type": "block_proposing",
            "regex": r'^(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2}\.\d{3})  INFO.*sc_basic_authorship::basic_authorship: 🎁 Prepared block for proposing at \d+ \(\d+ ms\).* extrinsics \((\d+)\)',
            "guard": "Prepared block for proposing",
            "column_names": ["date", "time", "extrinsics_count"],
            "extract_data": lambda match: (
                    match.group(1),
                    match.group(2),
                    match.group(3)
                )
        },
        {
            "type": "block_proposing_start",
            "regex": r'^(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2}\.\d{3})  INFO.*Starting consensus session on top of parent.*',
            "guard": "Starting consensus session on top of parent",
            "column_names": ["date", "time", "value"],
            "extract_data": lambda match: (
                    match.group(1),
                    match.group(2),
                    0
                )
        }
         
    ]
    
    log_file_path = sys.argv[1]
    output_dir = sys.argv[2]
    ensure_dir_exists(output_dir)

    parsed_data = parse_log_file(patterns, log_file_path)
    for key, value in parsed_data.items():
        save_parsed_data(value['data'], value['columns'], f"{output_dir}/{key}.csv")

if __name__ == "__main__":
    main()

