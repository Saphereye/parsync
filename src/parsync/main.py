#!/usr/bin/env python3
"""
Rsync - Intelligent rsync wrapper with optimized parallelization
"""

import argparse
import logging
import shlex
import sys
from typing import List

from rich.console import Console

from .wrapper import RsyncWrapper

console = Console()


def create_parser() -> argparse.ArgumentParser:
    """Create argument parser"""
    parser = argparse.ArgumentParser(
        description="rsync wrapper with intelligent parallelization",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  parsync /source/ /destination/
  parsync -t 4 /local/path/ user@remote:/path/
  parsync --rsync-args "-av --exclude=*.tmp" /src/ /dst/
  parsync -c 500 /large_files/ /backup/
        """,
    )
    
    parser.add_argument("source", help="Source path")
    parser.add_argument("destination", help="Destination path")
    parser.add_argument(
        "-t", "--threads", 
        type=int, 
        help="Maximum number of threads (auto-detected if not specified)"
    )
    parser.add_argument(
        "-c", "--chunk-size", 
        type=int, 
        default=100, 
        help="Chunk size in MB (default: 100)"
    )
    parser.add_argument(
        "-v", "--verbose", 
        action="store_true", 
        help="Verbose output"
    )
    parser.add_argument(
        "--rsync-args", 
        help="Additional rsync arguments (quoted string)"
    )
    parser.add_argument(
        "--dry-run", 
        action="store_true", 
        help="Show what would be done without executing"
    )
    parser.add_argument(
        "--version", 
        action="version", 
        version="%(prog)s 0.1.0"
    )
    
    return parser


def parse_rsync_args(rsync_args_str: str) -> List[str]:
    """Parse rsync arguments from string"""
    if not rsync_args_str:
        return ['-av', '--progress']
    
    try:
        return shlex.split(rsync_args_str)
    except ValueError as e:
        console.print(f"[red]Error parsing rsync arguments: {e}[/red]")
        sys.exit(1)


def main() -> None:
    """Main entry point"""
    parser = create_parser()
    args = parser.parse_args()
    
    # Configure logging
    if args.verbose:
        logging.basicConfig(
            level=logging.DEBUG,
            format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
        )
    else:
        logging.basicConfig(level=logging.WARNING)
    
    # Parse rsync arguments
    rsync_args = parse_rsync_args(args.rsync_args)
    
    # Add dry-run if requested
    if args.dry_run:
        rsync_args.append('--dry-run')
        console.print("[yellow]DRY RUN MODE - No files will be transferred[/yellow]")
    
    try:
        # Create and run wrapper
        wrapper = RsyncWrapper(
            source=args.source,
            destination=args.destination,
            rsync_args=rsync_args,
            max_threads=args.threads,
            chunk_size_mb=args.chunk_size
        )
        
        success = wrapper.run()
        sys.exit(0 if success else 1)
        
    except KeyboardInterrupt:
        console.print("\n[yellow]Transfer interrupted by user[/yellow]")
        sys.exit(130)
    except Exception as e:
        console.print(f"[red]Unexpected error: {e}[/red]")
        if args.verbose:
            import traceback
            traceback.print_exc()
        sys.exit(1)


if __name__ == "__main__":
    main()
