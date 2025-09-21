#!/bin/bash
# pgpkms wrapper to make it compatible with RPM's GPG interface
# This script translates RPM's GPG arguments to pgpkms format

# Debug: log all arguments to stderr
echo "pgpkms-gpg-wrapper called with args: $*" >&2

# Parse arguments to find the input file
input_file=""
detach_sign=false
armor=false
local_user=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --detach-sign)
            detach_sign=true
            shift
            ;;
        --armor)
            armor=true
            shift
            ;;
        --local-user)
            local_user="$2"
            shift 2
            ;;
        -u)
            local_user="$2"
            shift 2
            ;;
        --batch|--no-tty|--pinentry-mode|--passphrase-fd)
            # Skip these GPG-specific options
            shift
            if [[ "$1" != -* ]] && [[ -n "$1" ]]; then
                shift
            fi
            ;;
        --*)
            # Skip other options
            shift
            ;;
        *)
            # This should be the input file
            if [[ -z "$input_file" ]] && [[ -f "$1" ]]; then
                input_file="$1"
            fi
            shift
            ;;
    esac
done

if [[ -z "$input_file" ]]; then
    echo "Error: No input file found" >&2
    exit 1
fi

echo "Signing file: $input_file" >&2

# Call pgpkms with the appropriate arguments
if [[ "$armor" == "true" ]]; then
    exec /home/runner/.local/bin/pgpkms sign --input "$input_file"
else
    exec /home/runner/.local/bin/pgpkms sign --input "$input_file" --binary
fi