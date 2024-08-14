#!/usr/bin/env python3
import re
import sys
import os
import subprocess
import argparse

def extract_time_point(command, file_path):
    result = subprocess.run(command, shell=True, capture_output=True, text=True)
    time_point = result.stdout.strip()
    with open(file_path, 'w') as f:
        f.write(time_point)
        print(f"{file_path}: {time_point}")

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
    path = os.path.join(os.getcwd(), dir_name)
    if not os.path.exists(path):
        os.makedirs(path)
        print(f"Directory '{dir_name}' was created.")
    else:
        print(f"Directory '{dir_name}' already exists.")

def parse_line(line, patterns):
    if "[Relaychain]" in line:
        return None

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
    print(f"Extracted data to: {output_filepath} data len: {len(data)}");
    with open(output_filepath, 'w') as file:
        file.write("\t".join(columns) + "\n")
        for entry in data:
            file.write("\t".join(map(str, entry)) + "\n")

def sanitize_string(string):
    return re.sub(r'[^A-Za-z0-9_]+', '_', string)

def longest_valid_substring(string):
    words = re.split(r'[^A-Za-z0-9_ ]+', string)
    return max(words, key=len)

def create_pattern(string):
    regex = r'^(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2}\.\d{3})'
    ptype = sanitize_string(string)
    guard = longest_valid_substring(string)
    return {
            "type": f"{ptype}",
            "regex": regex + f".*{string}",
            "guard": f"{guard}",
            "column_names": ["date", "time", "value"],
            "extract_data": lambda match: (
                match.group(1),
                match.group(2),
                1
                )
            }

def parse_temporary_patterns(output_dir, log_file, tmp_patterns):
    if tmp_patterns is None or len(tmp_patterns)==0:
        return None

    patterns = [create_pattern(string) for string in tmp_patterns]
    return patterns

def main():
    base_patterns = [
        {
            "type": "validate_transaction",
            "regex": r'(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2}\.\d{3}) DEBUG.*\[([0-9a-fx]+)\].*validate_transaction_blocking: at:.*took:(\d+\.\d+)(µs|ms)',
            "guard": "validate_transaction_blocking:",
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
            "regex": r'(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2}(?:\.\d{3})?).* Imported #(\d+) \((0x[a-f0-9…]+) → (0x[a-f0-9…]+)\)',
            "guard": "Imported #",
            "column_names": ["date", "time", "block_number"],
            "extract_data": lambda match: (
                    match.group(1),
                    match.group(2),
                    match.group(3),
                )
        },
        {
            "type": "txpool_maintain",
            "regex": "(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2}(?:\.\d{3})?).*maintain: txs:\((\d+), (\d+)\) views:\[(\d+);.*\] event:(NewBlock|NewBestBlock|Finalized) {.*}  took:(\d+\.\d+)([µms]+)",
            "guard": "maintain: txs:",
            "column_names": ["date", "time", "unwatched_txs", "watched_txs", "views_count", "event", "duration"],
            "extract_data": lambda match: (
                    match.group(1),
                    match.group(2),
                    match.group(3),
                    match.group(4),
                    match.group(5),
                    2 if match.group(6) == "Finalized" else 1 if match.group(6) == "NewBestBlock" else 0,
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
            "regex": r'^(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2}(?:\.\d{3})?).*🎁 Prepared block for proposing at \d+ \(\d+ ms\).* extrinsics_count: (\d+)',
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
            "regex": r'^(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2}(?:\.\d{3})?).*Starting consensus session on top of parent.*',
            "guard": "Starting consensus session on top of parent",
            "column_names": ["date", "time", "value"],
            "extract_data": lambda match: (
                    match.group(1),
                    match.group(2),
                    0
                )
        },
        {
            "type": "submit_one",
            "regex": r'^(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2}\.\d{3}) DEBUG.*fatp::submit_one.*',
            "guard": "fatp::submit_one",
            "column_names": ["date", "time", "value"],
            "extract_data": lambda match: (
                    match.group(1),
                    match.group(2),
                    1
                )
        },
        {
            "type": "propagate_transaction_failure",
            "regex": r'^(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2}\.\d{3}) DEBUG.*Propagating transaction failure',
            "guard": "Propagating transaction failure",
            "column_names": ["date", "time", "value"],
            "extract_data": lambda match: (
                    match.group(1),
                    match.group(2),
                    1
                )
        }
    ]

    parser = argparse.ArgumentParser(description='Parse substrate log.')
    parser.add_argument('log_file', help='Path to the log file')
    parser.add_argument('-r', action='append', dest='tmp_patterns', help='tmp patterns that can be presented on graph')

    args = parser.parse_args()
    
    log_file_path = args.log_file
    output_dir = os.path.splitext(os.path.basename(log_file_path))[0]
    print("Output dir is: ", output_dir)
    ensure_dir_exists(output_dir)

    tmp_patterns = parse_temporary_patterns(output_dir, args.log_file, args.tmp_patterns)

    patterns = base_patterns if tmp_patterns is None or len(tmp_patterns)==0 else tmp_patterns
    # print(patterns)

    parsed_data = parse_log_file(patterns, log_file_path)
    for key, value in parsed_data.items():
        save_parsed_data(value['data'], value['columns'], f"{output_dir}/{key}.csv")

    start_file = f"{output_dir}/../start"
    end_file = f"{output_dir}/../end"

    timestamp_command = f"grep '.*maintain.*took' {log_file_path} | head -n 1 | cut -f1,2 -d' ' | cut -f1 -d'.'"
    if not os.path.isfile(start_file):
        extract_time_point(timestamp_command, start_file)

    timestamp_command = f"grep '.*maintain.*took' {log_file_path} | tail -n 1 | cut -f1,2 -d' ' | cut -f1 -d'.'"
    if not os.path.isfile(end_file):
        extract_time_point(timestamp_command, end_file)

if __name__ == "__main__":
    main()

