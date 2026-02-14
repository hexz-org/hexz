"""Analysis utilities for benchmark results."""

import json
from pathlib import Path
from typing import Dict, List

from tabulate import tabulate


def load_results(results_dir: Path) -> Dict[str, List[Dict]]:
    """Load all benchmark results from JSON files."""
    results = {}
    for result_file in results_dir.glob("*_results.json"):
        format_name = result_file.stem.replace("_results", "")
        with open(result_file) as f:
            results[format_name] = json.load(f)
    return results


def generate_comparison_table(
    results: Dict[str, List[Dict]], test_name: str, include_all_metrics: bool = False
) -> str:
    """Generate comparison table for a specific test."""
    rows = []

    for format_name, test_results in sorted(results.items()):
        test_result = next(
            (r for r in test_results if r["test_name"] == test_name), None
        )

        if test_result:
            row = [format_name.upper()]

            if test_name == "storage_efficiency":
                row.append(f"{test_result.get('storage_mb', 0):.1f} MB")
                row.append(f"{test_result.get('compression_ratio', 1.0):.2f}x")
                metadata = test_result.get("metadata", {})
                row.append(f"{metadata.get('space_saved_percent', 0):.1f}%")
                row.append(f"{test_result.get('pack_time_s', 0):.2f}s")
                row.append(f"{test_result.get('pack_throughput_mb_s', 0):.1f} MB/s")
            else:
                if "throughput_mb_s" in test_result:
                    row.append(f"{test_result['throughput_mb_s']:.1f} MB/s")
                else:
                    row.append("N/A")

                if "latency_us" in test_result:
                    latency = test_result["latency_us"]
                    if latency > 1000:
                        row.append(f"{latency / 1000:.1f} ms")
                    else:
                        row.append(f"{latency:.1f} µs")
                else:
                    row.append("N/A")

                if "samples_per_sec" in test_result:
                    row.append(f"{test_result['samples_per_sec']:.0f}")
                else:
                    row.append("N/A")

                if include_all_metrics and "cpu_percent" in test_result:
                    row.append(f"{test_result['cpu_percent']:.1f}%")
                elif include_all_metrics:
                    row.append("N/A")

            rows.append(row)

    if test_name == "storage_efficiency":
        headers = [
            "Format",
            "Storage Size",
            "Compression",
            "Space Saved",
            "Pack Time",
            "Pack Speed",
        ]
    else:
        headers = ["Format", "Throughput", "Latency", "Samples/sec"]
        if include_all_metrics:
            headers.append("CPU Usage")

    return tabulate(rows, headers=headers, tablefmt="pipe")


def generate_summary_table(results: Dict[str, List[Dict]]) -> str:
    """Generate overall summary table with key metrics."""
    rows = []

    for format_name, test_results in sorted(results.items()):
        storage_result = next(
            (r for r in test_results if r["test_name"] == "storage_efficiency"), None
        )
        seq_result = next(
            (r for r in test_results if r["test_name"] == "sequential_read"), None
        )
        random_result = next(
            (r for r in test_results if r["test_name"] == "random_read"), None
        )

        row = [format_name.upper()]

        if storage_result:
            row.append(f"{storage_result.get('storage_mb', 0):.1f} MB")
            row.append(f"{storage_result.get('compression_ratio', 1.0):.2f}x")
        else:
            row.extend(["N/A", "N/A"])

        if seq_result:
            row.append(f"{seq_result.get('throughput_mb_s', 0):.1f}")
        else:
            row.append("N/A")

        if random_result:
            latency = random_result.get("latency_us", 0)
            if latency > 1000:
                row.append(f"{latency / 1000:.1f} ms")
            else:
                row.append(f"{latency:.1f} µs")
        else:
            row.append("N/A")

        rows.append(row)

    headers = ["Format", "Storage", "Compression", "Sequential", "Random Latency"]
    return tabulate(rows, headers=headers, tablefmt="pipe")


def find_winner(
    results: Dict[str, List[Dict]],
    test_name: str,
    metric: str,
    lower_is_better: bool = False,
) -> Dict[str, float]:
    """Find the best performer for a specific metric."""
    best_value = None
    best_format = None

    for format_name, test_results in results.items():
        test_result = next(
            (r for r in test_results if r["test_name"] == test_name), None
        )
        if test_result and metric in test_result:
            value = test_result[metric]
            if best_value is None:
                best_value = value
                best_format = format_name
            elif lower_is_better and value < best_value:
                best_value = value
                best_format = format_name
            elif not lower_is_better and value > best_value:
                best_value = value
                best_format = format_name

    if best_format:
        return {"format": best_format, "value": best_value}
    return {}


def generate_markdown_report(results: Dict[str, List[Dict]], output_file: Path):
    """Generate comprehensive markdown comparison report."""
    from .report_templates import (
        generate_header,
        generate_executive_summary,
        generate_detailed_sections,
        generate_recommendations,
    )

    lines = []
    lines.extend(generate_header())
    lines.extend(generate_executive_summary(results))
    lines.extend(generate_detailed_sections(results))
    lines.extend(generate_recommendations())

    with open(output_file, "w") as f:
        f.write("\n".join(lines))

    print(f"✅ Report saved to: {output_file}")


def print_summary(results: Dict[str, List[Dict]]):
    """Print summary to console."""
    print("\n" + "=" * 80)
    print("Benchmark Results Summary")
    print("=" * 80 + "\n")

    print(generate_summary_table(results))
    print()

    for format_name, test_results in sorted(results.items()):
        print(f"\n{format_name.upper()}")
        print("-" * len(format_name))

        for result in sorted(test_results, key=lambda x: x["test_name"]):
            print(f"\n  {result['test_name']}:")

            if result["test_name"] == "storage_efficiency":
                print(f"    Storage: {result.get('storage_mb', 0):.1f} MB")
                print(f"    Compression: {result.get('compression_ratio', 1.0):.2f}x")
                metadata = result.get("metadata", {})
                print(f"    Space saved: {metadata.get('space_saved_percent', 0):.1f}%")
                print(f"    Pack time: {result.get('pack_time_s', 0):.2f}s")
                print(
                    f"    Pack speed: {result.get('pack_throughput_mb_s', 0):.1f} MB/s"
                )
            else:
                if "throughput_mb_s" in result:
                    print(f"    Throughput: {result['throughput_mb_s']:.1f} MB/s")
                if "latency_us" in result:
                    latency = result["latency_us"]
                    if latency > 1000:
                        print(f"    Latency: {latency / 1000:.1f} ms")
                    else:
                        print(f"    Latency: {latency:.1f} µs")
                if "samples_per_sec" in result:
                    print(f"    Samples/sec: {result['samples_per_sec']:.0f}")
