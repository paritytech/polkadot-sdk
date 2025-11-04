#!/bin/bash
# pgpkms wrapper to make it compatible with RPM's GPG interface
# This script translates RPM's GPG arguments to pgpkms format

# Debug: log all arguments to stderr
echo "pgpkms-gpg-wrapper called with args: $*" >&2

# Parse arguments to find the input file and options
input_file=""
output_file=""
detach_sign=false
armor=false
local_user=""
read_from_stdin=false

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
        -sbo)
            # RPM uses -sbo which means: -s (sign), -b (detach), -o (output to file)
            detach_sign=true
            # The next argument should be the output file
            shift
            if [[ -n "$1" ]] && [[ "$1" != "--" ]]; then
                output_file="$1"
                shift
            fi
            ;;
        --no-verbose|--no-armor|--no-secmem-warning|--batch|--no-tty|--pinentry-mode|--passphrase-fd)
            # Skip these GPG-specific options
            shift
            ;;
        --)
            # End of options marker
            shift
            break
            ;;
        --*)
            # Skip other long options
            shift
            ;;
        -*)
            # Skip other short options
            shift
            ;;
        *)
            # This could be a file argument
            if [[ "$1" == "-" ]]; then
                read_from_stdin=true
            elif [[ -z "$input_file" ]] && [[ -f "$1" ]]; then
                input_file="$1"
            fi
            shift
            ;;
    esac
done

# Handle remaining arguments after --
while [[ $# -gt 0 ]]; do
    if [[ "$1" == "-" ]]; then
        read_from_stdin=true
    elif [[ -z "$input_file" ]] && [[ -f "$1" ]]; then
        input_file="$1"
    fi
    shift
done

echo "Parsed: input_file='$input_file', output_file='$output_file', read_from_stdin=$read_from_stdin, armor=$armor" >&2

# If we're supposed to read from stdin, we need to create a temp file
if [[ "$read_from_stdin" == "true" ]]; then
    temp_input=$(mktemp)
    cat > "$temp_input"
    input_file="$temp_input"
    echo "Created temp file for stdin: $input_file" >&2
fi

if [[ -z "$input_file" ]]; then
    echo "Error: No input file found" >&2
    exit 1
fi

echo "Signing file: $input_file" >&2

# Call pgpkms with the appropriate arguments
pgpkms_args="sign --input $input_file"

if [[ -n "$output_file" ]]; then
    pgpkms_args="$pgpkms_args --output $output_file"
fi

if [[ "$armor" != "true" ]]; then
    pgpkms_args="$pgpkms_args --binary"
fi

echo "Running: /home/runner/.local/bin/pgpkms $pgpkms_args" >&2
exec /home/runner/.local/bin/pgpkms $pgpkms_args