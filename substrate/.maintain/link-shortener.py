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
from typing import Set, Tuple, NamedTuple
import logging
from dataclasses import dataclass
from collections import defaultdict

# Configure logging
logging.basicConfig(level=logging.INFO, format='%(levelname)s: %(message)s')

@dataclass
class ReferenceEntry:
    """Represents a single reference entry with all its components."""
    short_name: str
    full_path: str
    anchor: str = ""
    display_text: str = ""

    def __hash__(self):
        return hash((self.short_name, self.full_path))

class DocLinkProcessor:
    """Processes documentation links in Rust files."""
    
    #Patterns for all link types
    PATTERNS = [
        # Basic crate references
        r'\[`crate::[^`]+`\]',
        r'\[`crate::reference_docs::[^`]+`\]',
        r'\[`crate::guides::[^`]+`\]',
        r'\[`crate::polkadot_sdk::[^`]+`\]',
        
        # Anchor links
        r'\[`[^`]+#[^`]+`\]',
        
        # Frame and pallet references
        r'\[`frame[^`]+`\]',
        r'\[`pallet[^`]+`\]',
        r'\[`sp_runtime[^`]+`\]',
        
        # Links with custom text
        r'\[[^\]]+\]\([^)]+\)'
    ]

    def __init__(self, dry_run: bool = False):
        self.dry_run = dry_run
        self.stats = {
            'processed': 0,
            'modified': 0,
            'errors': []
        }
        self.references = set()

    def parse_link(self, match) -> ReferenceEntry:
        """Parse a link match into its components."""
        full_match = match.group(0)
        
        # Handle links with custom text: [text](link)
        if ')' in full_match and '](' in full_match:
            display_text = full_match[1:full_match.index(']')]
            path = full_match[full_match.index('(')+1:-1].strip('`')
            base_path = path.split('#')[0]
            anchor = path.split('#')[1] if '#' in path else ""
            short_name = display_text.strip('`').split('::')[-1]
        else:
            # Handle direct links: [`path`] or [`path#anchor`]
            path = full_match[2:-2]  # Remove [` and `]
            if '#' in path:
                base_path, anchor = path.split('#', 1)
            else:
                base_path, anchor = path, ""
            
            short_name = base_path.split('::')[-1]

        return ReferenceEntry(
            short_name=short_name,
            full_path=base_path,
            anchor=anchor,
            display_text=display_text if ')' in full_match else ""
        )

    def format_reference(self, entry: ReferenceEntry) -> str:
        """Format a reference entry as a Rust reference line."""
        if entry.anchor:
            return f'// [`{entry.short_name}`]: {entry.full_path}#{entry.anchor}'
        return f'// [`{entry.short_name}`]: {entry.full_path}'

    def process_file(self, file_path: Path) -> None:
        """Process a single file to shorten documentation links."""
        try:
            self.stats['processed'] += 1
            self.references.clear()
            
            content = file_path.read_text()
            if not any(x in content for x in ['crate::', 'frame::', 'pallet_', 'sp_runtime::']):
                return

            modified_content = content

            # First pass: Collect all references
            for pattern in self.PATTERNS:
                def collect_references(match):
                    entry = self.parse_link(match)
                    self.references.add(entry)
                    return f'[`{entry.short_name}`]'

                modified_content = re.sub(pattern, collect_references, modified_content)

            if not self.references:
                return

            # Remove existing reference section
            modified_content = re.sub(r'\n// \[`[^`]+`\][^\n]*\n?', '\n', modified_content)
            modified_content = re.sub(r'\n//! [^\n]*\n?', '\n', modified_content)

            # Add new reference section
            modified_content = modified_content.rstrip('\n') + '\n\n'
            
            # Sort and deduplicate references
            sorted_refs = sorted(self.references, key=lambda x: (x.short_name, x.full_path))
            seen = set()
            for ref in sorted_refs:
                if ref.short_name not in seen:
                    modified_content += self.format_reference(ref) + '\n'
                    seen.add(ref.short_name)

            if content != modified_content:
                if not self.dry_run:
                    file_path.write_text(modified_content)
                    self.stats['modified'] += 1
                    logging.info(f"Modified {file_path}")
                else:
                    logging.info(f"Would modify {file_path}")

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

    target_dir = Path(args.path)
    if not target_dir.is_absolute():
        target_dir = Path.cwd() / args.path

    if not target_dir.exists():
        logging.error(f"Directory not found: {target_dir}")
        sys.exit(1)

    if args.dry_run:
        logging.info("Running in dry-run mode - no files will be modified")

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