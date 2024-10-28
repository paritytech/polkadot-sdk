#!/usr/bin/env python3

"""
Polkadot SDK Documentation Link Shortener

This script processes Rust documentation files and shortens crate reference links
to improve readability. It should be run from the polkadot-sdk root directory.

Examples:
    Test run (no changes made):
    $ python substrate/.maintain/link-shortener.py --dry-run

    Process all documentation:
    $ python substrate/.maintain/link-shortener.py

    Process specific directory:
    $ python substrate/.maintain/link-shortener.py --path docs/sdk/src/guides

Note: This script defaults to processing the docs/sdk/src directory from the
polkadot-sdk root directory.
"""

import re
import sys
import os
import argparse
from pathlib import Path
from typing import Set, Tuple
import logging

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(levelname)s: %(message)s'
)

class DocLinkProcessor:
    """Processes documentation links in Rust files."""
    
    # Patterns to match various documentation link formats
    PATTERNS = [
        r'\[`crate::reference_docs::[^`]+`\]',
        r'\[`crate::guides::[^`]+`\]',
        r'\[`crate::polkadot_sdk::[^`]+`\]'
    ]

    def __init__(self, dry_run: bool = False):
        self.dry_run = dry_run
        self.stats = {
            'processed': 0,
            'modified': 0,
            'errors': []
        }

    def process_file(self, file_path: Path) -> None:
        """Process a single file to shorten documentation links."""
        try:
            self.stats['processed'] += 1
            
            # Read file content
            content = file_path.read_text()
            
            # Skip if no potential links
            if "crate::" not in content:
                return

            links: Set[Tuple[str, str]] = set()
            modified_content = content

            # Process each pattern
            for pattern in self.PATTERNS:
                def replacer(match: re.Match) -> str:
                    full_path = match.group(0)  # [`crate::path::to::item`]
                    path_without_brackets = full_path[2:-2]
                    short_name = path_without_brackets.split("::")[-1]
                    links.add((short_name, path_without_brackets))
                    return f'[`{short_name}`]({path_without_brackets})'

                modified_content = re.sub(pattern, replacer, modified_content)

            if not links:
                return

            # Add reference section
            if not modified_content.endswith('\n'):
                modified_content += '\n'
            
            modified_content += '\n// Link References\n'
            for short_name, full_path in sorted(links):
                modified_content += f'// [`{short_name}`]: {full_path}\n'

            # Handle dry run
            if self.dry_run:
                logging.info(f"Would modify {file_path}:")
                logging.info("---")
                logging.info(modified_content)
                logging.info("---")
                return

            # Write changes
            file_path.write_text(modified_content)
            self.stats['modified'] += 1
            logging.info(f"Modified {file_path}")

        except Exception as e:
            error_msg = f"Error processing {file_path}: {str(e)}"
            self.stats['errors'].append(error_msg)
            logging.error(error_msg)

    def print_summary(self) -> None:
        """Print processing summary."""
        print("\nSummary:")
        print(f"Files processed: {self.stats['processed']}")
        print(f"Files modified: {self.stats['modified']}")
        
        if self.stats['errors']:
            print(f"\nErrors encountered: {len(self.stats['errors'])}")
            for error in self.stats['errors']:
                print(f"- {error}")

def get_project_root() -> Path:
    """Get the project root directory."""
    # Assuming script is run from project root or script location
    script_dir = Path(__file__).resolve().parent
    if script_dir.name == '.maintain':
        # If we're in .maintain, go up two levels to get to project root
        return script_dir.parent.parent
    return script_dir

def main():
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument('--dry-run', action='store_true',
                      help="Show what would be changed without making changes")
    parser.add_argument('--path', default='docs/sdk/src',
                      help="Path to process (default: docs/sdk/src)")
    parser.add_argument('-v', '--verbose', action='store_true',
                      help="Enable verbose output")

    args = parser.parse_args()

    if args.verbose:
        logging.getLogger().setLevel(logging.DEBUG)

    # Get project root and target directory
    project_root = get_project_root()
    target_dir = project_root / args.path

    if not target_dir.exists():
        logging.error(f"Directory not found: {target_dir}")
        sys.exit(1)

    if args.dry_run:
        logging.info("Running in dry-run mode - no files will be modified")

    # Process files
    processor = DocLinkProcessor(dry_run=args.dry_run)
    
    try:
        for file_path in target_dir.rglob('*.rs'):
            processor.process_file(file_path)
        
        processor.print_summary()
        
        if processor.stats['errors']:
            sys.exit(1)
            
    except KeyboardInterrupt:
        logging.info("\nOperation cancelled by user")
        sys.exit(1)
    except Exception as e:
        logging.error(f"Fatal error: {e}")
        sys.exit(1)

if __name__ == '__main__':
    main()