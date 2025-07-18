#!/bin/bash

TARGET_FILE="umbrella/Cargo.toml"
TMP_FILE="${TARGET_FILE}.tmp"

echo "Processing $TARGET_FILE..."

# Find and remove version lines in [dependencies.*] sections only
awk '
  # Match [dependencies.<crate>] section
  /^\[dependencies\.[^]]+\]/ {
    in_dependencies_section = 1
    print
    next
  }

  # Any new section turns off the flag
  /^\[.*\]/ {
    in_dependencies_section = 0
    print
    next
  }

  # Skip version = "..." if in a [dependencies.*] section
  {
    if (in_dependencies_section && $0 ~ /^[ \t]*version[ \t]*=[ \t]*".*"/) {
      next
    } else {
      print
    }
  }
' "$TARGET_FILE" > "$TMP_FILE" && mv "$TMP_FILE" "$TARGET_FILE"

echo "✅ Done: Removed version lines inside [dependencies.*] sections."
