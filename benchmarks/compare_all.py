#!/usr/bin/env python3
"""
Compare all benchmark results and generate comparison tables.

Reads JSON results from benchmarks/results/ and creates comparison tables.
"""

import argparse
import json
from pathlib import Path
from typing import Dict

from tabulate import tabulate


def load_results(results_dir: Path) -> Dict[str, Dict]:
    """Load all benchmark results from JSON files."""
    results = {}

    for result_file in results_dir.glob("*_results.json"):
        format_name = result_file.stem.replace("_results", "")

        with open(result_file) as f:
            data = json.load(f)
            results[format_name] = data

    return results


def compare_throughput(results: Dict[str, Dict]) -> str:
    """Generate throughput comparison table."""
    headers = ["Format", "Write (GB/s)", "Sequential Read (GB/s)", "Storage (GB)"]
    rows = []

    for format_name, data in sorted(results.items()):
        metrics = data.get("metrics", {})

        write_gbps = metrics.get("write_throughput_gbps", 0)
        seq_read_gbps = metrics.get("sequential_read_throughput_gbps", 0)
        storage_gb = metrics.get("total_gb", 0)

        rows.append(
            [
                format_name,
                f"{write_gbps:.2f}",
                f"{seq_read_gbps:.2f}",
                f"{storage_gb:.2f}",
            ]
        )

    return tabulate(rows, headers=headers, tablefmt="pipe")


def compare_latency(results: Dict[str, Dict]) -> str:
    """Generate latency comparison table."""
    headers = ["Format", "Cold Access (µs)", "Warm Access (µs)", "P95 Cold (µs)"]
    rows = []

    for format_name, data in sorted(results.items()):
        metrics = data.get("metrics", {})

        # Try different naming conventions
        cold_mean = metrics.get("random_access_cold_mean_us") or metrics.get(
            "random_access_mean_us", 0
        )
        warm_mean = metrics.get("random_access_warm_mean_us", 0)
        cold_p95 = metrics.get("random_access_cold_p95_us") or metrics.get(
            "random_access_p95_us", 0
        )

        rows.append(
            [
                format_name,
                f"{cold_mean:.0f}" if cold_mean else "N/A",
                f"{warm_mean:.0f}" if warm_mean else "N/A",
                f"{cold_p95:.0f}" if cold_p95 else "N/A",
            ]
        )

    return tabulate(rows, headers=headers, tablefmt="pipe")


def compare_efficiency(results: Dict[str, Dict]) -> str:
    """Generate storage efficiency comparison."""
    headers = ["Format", "Size (GB)", "Compression Ratio", "Format Details"]
    rows = []

    # Get raw size from local_files if available
    raw_size_gb = None
    if "local_files" in results:
        raw_size_gb = results["local_files"]["metrics"].get("total_gb")

    for format_name, data in sorted(results.items()):
        metrics = data.get("metrics", {})
        storage_gb = metrics.get("total_gb", 0)

        # Calculate compression ratio if we have raw size
        if raw_size_gb and raw_size_gb > 0:
            ratio = raw_size_gb / storage_gb
        else:
            ratio = None

        # Format-specific details
        details = []
        if format_name == "webdataset":
            num_shards = metrics.get("num_shards", "?")
            details.append(f"{num_shards} shards")
        elif format_name == "hdf5":
            compression = metrics.get("compression", "?")
            details.append(f"{compression} compression")

        rows.append(
            [
                format_name,
                f"{storage_gb:.2f}",
                f"{ratio:.2f}x" if ratio else "N/A",
                ", ".join(details) if details else "-",
            ]
        )

    return tabulate(rows, headers=headers, tablefmt="pipe")


def generate_summary(results: Dict[str, Dict]) -> str:
    """Generate overall summary."""
    summary = []
    summary.append("# Benchmark Results Summary\n")
    summary.append(f"**Compared formats:** {', '.join(sorted(results.keys()))}\n")

    # System info (from first result)
    if results:
        first_result = next(iter(results.values()))
        sys_info = first_result.get("system", {})
        summary.append("\n**Test System:**")
        summary.append(f"- CPU: {sys_info.get('cpu', 'Unknown')}")
        summary.append(f"- CPU cores: {sys_info.get('cpu_count', '?')}")
        summary.append(f"- RAM: {sys_info.get('ram_gb', '?'):.1f} GB")
        summary.append(f"- Platform: {sys_info.get('platform', 'Unknown')}\n")

    return "\n".join(summary)


def main():
    parser = argparse.ArgumentParser(description="Compare benchmark results")
    parser.add_argument(
        "--results-dir",
        type=Path,
        default=Path("benchmarks/results"),
        help="Directory containing benchmark results",
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="Output markdown file (default: print to stdout)",
    )

    args = parser.parse_args()

    # Load results
    results = load_results(args.results_dir)

    if not results:
        print(f"No results found in {args.results_dir}")
        return

    print(f"Loaded {len(results)} benchmark results")

    # Generate comparison tables
    output = []

    output.append(generate_summary(results))

    output.append("## Throughput Comparison\n")
    output.append(compare_throughput(results))
    output.append("")

    output.append("## Latency Comparison\n")
    output.append(compare_latency(results))
    output.append("")

    output.append("## Storage Efficiency\n")
    output.append(compare_efficiency(results))
    output.append("")

    output.append("## Notes\n")
    output.append("- All benchmarks run on identical test data")
    output.append("- Cold access = first iteration, Warm access = cached")
    output.append("- Compression ratio = raw size / compressed size")

    output_text = "\n".join(output)

    # Output results
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        with open(args.output, "w") as f:
            f.write(output_text)
        print(f"\nComparison saved to: {args.output}")
    else:
        print("\n" + output_text)


if __name__ == "__main__":
    main()
