"""Report templates for markdown generation."""

from typing import Dict, List

from .analysis import (
    find_winner,
    generate_summary_table,
    generate_comparison_table,
)


def generate_header() -> List[str]:
    """Generate report header."""
    return [
        "# Benchmark Results Comparison",
        "",
        "Comprehensive comparison of Hexz against popular ML data formats.",
        "",
        "**Auto-generated from benchmark runs**",
        "",
        "---",
        "",
    ]


def generate_executive_summary(results: Dict[str, List[Dict]]) -> List[str]:
    """Generate executive summary section."""
    lines = ["## Executive Summary", "", generate_summary_table(results), ""]
    lines.append("**Key Takeaways:**")
    lines.append("")

    # Find winners
    storage_winner = find_winner(
        results, "storage_efficiency", "storage_mb", lower_is_better=True
    )
    compression_winner = find_winner(
        results, "storage_efficiency", "compression_ratio", lower_is_better=False
    )
    seq_winner = find_winner(
        results, "sequential_read", "throughput_mb_s", lower_is_better=False
    )
    random_winner = find_winner(
        results, "random_read", "latency_us", lower_is_better=True
    )

    if storage_winner:
        lines.append(
            f"- 🏆 **Smallest storage**: {storage_winner['format']} ({storage_winner['value']:.1f} MB)"
        )
    if compression_winner:
        lines.append(
            f"- 🏆 **Best compression**: {compression_winner['format']} ({compression_winner['value']:.2f}x)"
        )
    if seq_winner:
        lines.append(
            f"- 🏆 **Fastest sequential**: {seq_winner['format']} ({seq_winner['value']:.1f} MB/s)"
        )
    if random_winner:
        latency = random_winner["value"]
        if latency > 1000:
            lines.append(
                f"- 🏆 **Fastest random access**: {random_winner['format']} ({latency / 1000:.1f} ms)"
            )
        else:
            lines.append(
                f"- 🏆 **Fastest random access**: {random_winner['format']} ({latency:.1f} µs)"
            )

    lines.extend(["", "---", ""])
    return lines


def generate_detailed_sections(results: Dict[str, List[Dict]]) -> List[str]:
    """Generate detailed test sections."""
    lines = []

    test_order = [
        "storage_efficiency",
        "sequential_read",
        "random_read",
        "shuffled_epoch",
    ]
    test_titles = {
        "storage_efficiency": "Storage Efficiency & Compression",
        "sequential_read": "Sequential Read Performance",
        "random_read": "Random Access Performance",
        "shuffled_epoch": "Shuffled Epoch (Training Simulation)",
    }

    # Get all test names
    test_names = set()
    for test_results in results.values():
        for result in test_results:
            test_names.add(result["test_name"])

    for test_name in test_order:
        if test_name not in test_names:
            continue

        lines.append(
            f"## {test_titles.get(test_name, test_name.replace('_', ' ').title())}"
        )
        lines.append("")
        lines.append(
            generate_comparison_table(results, test_name, include_all_metrics=True)
        )
        lines.append("")

    return lines


def generate_recommendations() -> List[str]:
    """Generate recommendations section."""
    return [
        "---",
        "",
        "## Recommendations",
        "",
        "### When to Use Each Format",
        "",
        "**Local Files**:",
        "- ✅ Small datasets (<10GB) that fit on local disk",
        "- ✅ Maximum simplicity and compatibility",
        "- ❌ No compression (expensive on S3)",
        "",
        "**HDF5**:",
        "- ✅ Mature ecosystem and tooling",
        "- ✅ Good compression and random access",
        "- ❌ Complex API for simple use cases",
        "",
        "**WebDataset**:",
        "- ✅ Excellent for sequential streaming",
        "- ✅ PyTorch integration",
        "- ❌ Shard-limited shuffling (not true random)",
        "- ❌ Very slow random access",
        "",
        "**Hexz**:",
        "- ✅ Fast random access (ideal for shuffling)",
        "- ✅ Good compression ratios",
        "- ✅ S3/HTTP streaming support",
        "- ⚠️ Newer format (less mature ecosystem)",
        "",
    ]
