#!/usr/bin/env bash
echo "Running script relative to `pwd`"
# Find all README.docify.md files
DOCIFY_FILES=$(find . -name "README.docify.md")

# Initialize a variable to track directories needing README regeneration
NEED_REGENERATION=""

for file in $DOCIFY_FILES; do
  echo "Processing $file"
  
  # Get the directory containing the docify file
  DIR=$(dirname "$file")
  
  # Go to the directory and run cargo build
  cd "$DIR"
  cargo check --features generate-readme || { echo "Readme generation for $DIR failed. Ensure the crate compiles successfully and has a `generate-readme` feature which guards markdown compilation in the crate as follows: https://docs.rs/docify/latest/docify/macro.compile_markdown.html#conventions." && exit 1; }
  
  # Check if README.md has any uncommitted changes
  git diff --exit-code README.md
  
  if [ $? -ne 0 ]; then
    echo "Error: Found uncommitted changes in $DIR/README.md"
    NEED_REGENERATION="$NEED_REGENERATION $DIR"
  fi
  
  # Return to the original directory
  cd - > /dev/null
done

# Check if any directories need README regeneration
if [ -n "$NEED_REGENERATION" ]; then
  echo "The following directories need README regeneration:"
  echo "$NEED_REGENERATION"
  exit 1
fi