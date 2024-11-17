#!/usr/bin/env python3

"""
Enhanced Polkadot SDK Documentation Link Shortener

This script processes Rust documentation files and shortens crate reference links
to improve readability. It should be run from the polkadot-sdk root directory.

Examples:
    Test run (no changes made):
    $ python substrate/.maintain/link-shortener.py --dry-run

    Process all documentation:
    $ python substrate/.maintain/link-shortener.py

    Process specific directory:
    $ python substrate/.maintain/link-shortener.py --path docs/sdk/src/guides

Note: The script defaults to processing the docs/sdk/src directory from the
polkadot-sdk root directory.
"""

import re
import sys
import os
import argparse
from pathlib import Path
from typing import Set, Tuple, Dict
import logging

# Configure logging
logging.basicConfig(level=logging.INFO, format='%(levelname)s: %(message)s')

class DocLinkProcessor:
    """Processes documentation links in Rust files."""
    
    # Patterns to match various documentation link formats
    PATTERNS = [
        r'\[`crate::[^`]+`\]',
        r'\[`crate::reference_docs::[^`]+`\]',
        r'\[`crate::guides::[^`]+`\]',
        r'\[`crate::polkadot_sdk::[^`]+`\]'
        r'\[`crate::\w+::[^`#]+(?:#[^`]+)?`\]' 
        r'\[`[^`]+`\]\(`crate::[^`]+`\)',
        r'\[`[^`]+`\]\(crate::[^)]+\)', # Added to catch links with anchors (#)

        # New frame patterns
        r'\[`frame[^`]+`\]',  # For frame system links
        r'\[`pallet[^`]+`\]', # For pallet links
        r'\[`sp_runtime[^`]+`\]'  
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
            if not any(x in content for x in ['crate::', 'frame::', 'pallet_', 'sp_runtime::']):
                return

            links: Set[Tuple[str, str]] = set()
            modified_content = content

            # Process each pattern
            for pattern in self.PATTERNS:
                def replacer(match):
                    full_path = match.group(0)
                    
                    if ')' in full_path and '](' in full_path:
                        name_part = full_path.split('](')[0][2:-1]  # Extract name between [` and `]
                        path_part = full_path.split('](')[1][:-1]   # Extract path between ( and )
                        short_name = name_part
                        path_without_brackets = path_part
                    else:
                        # Handle direct links: [`path`]
                        path_without_brackets = full_path[2:-2] 
                        if '#' in path_without_brackets:
                            # For links with anchors, keep full reference
                            parts = path_without_brackets.replace('\n//! ', '').split('#', 1)
                            base_path = parts[0]
                            anchor = parts[1] if len(parts) > 1 else ''
                            short_name = base_path.split('::')[-1]
                            # Keep the full reference including anchor without newlines
                            path_without_brackets = f"{base_path}#{anchor}"
                        else:                  
                            if path_without_brackets.startswith('crate::'):
                                short_name = path_without_brackets.split('::')[-1]
                            else:
                                short_name = path_without_brackets.split('::')[-1] if '::' in path_without_brackets else path_without_brackets

                    # Verify we have both values before proceeding
                    if short_name is None or path_without_brackets is None:
                        print(f"Warning: Could not process link {full_path}")
                        return full_path     

                    # Add to links set for reference section
                    links.add((short_name, path_without_brackets))
                    print(f"Found link: {full_path}")
                    print(f"Converting to: [`{short_name}`]")
                    
                    return f'[`{short_name}`]'

                modified_content = re.sub(pattern, replacer, modified_content)

            if not links:
                return

            # Add reference section
            if not modified_content.endswith('\n\n'):
                modified_content =  modified_content.rstrip('\n') + '\n\n'

            # modified_content += '\n'
            for short_name, full_path in sorted(links):
                    clean_path = full_path.replace('\n//! ', '')
                    modified_content += f'// [`{short_name}`]: {clean_path}\n'
                
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

    # Get target directory
    target_dir = Path(args.path)
    if not target_dir.is_absolute():
        target_dir = Path.cwd() / args.path

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