#!/usr/bin/env python3
"""CLI script to analyze benchmark results."""

import argparse
import sys
from pathlib import Path

# Add src to path
sys.path.insert(0, str(Path(__file__).parent / "src"))

from benchmarks.utils.analysis import (
    load_results,
    generate_markdown_report,
    print_summary,
)


def main():
    parser = argparse.ArgumentParser(description="Analyze benchmark results")
    parser.add_argument(
        "--results-dir",
        type=Path,
        default=Path(__file__).parent / "results",
        help="Directory containing benchmark results",
    )
    parser.add_argument("--output", type=Path, help="Output markdown file (optional)")

    args = parser.parse_args()

    if not args.results_dir.exists():
        print(f"❌ Results directory not found: {args.results_dir}")
        print("\nPlease run benchmarks first: python run_benchmarks.py")
        return

    print(f"Loading results from {args.results_dir}...")
    results = load_results(args.results_dir)

    if not results:
        print("❌ No results found!")
        return

    print(f"Found results for {len(results)} formats:")
    for format_name in sorted(results.keys()):
        print(f"  • {format_name}")

    print_summary(results)

    if args.output:
        generate_markdown_report(results, args.output)
    else:
        print("\n" + "=" * 80)
        print("To generate markdown report:")
        print("  python analyze_results.py --output report.md")


if __name__ == "__main__":
    main()
