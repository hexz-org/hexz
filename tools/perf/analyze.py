#!/usr/bin/env python3
"""Analyze profiling data and display a clean terminal report.

Supports two input formats:
  .stacks  — collapsed stacks (py-spy --format raw)
  .perf    — perf script output (parsed into collapsed stacks internally)

Usage:
    python analyze.py [--top N] [--threshold PCT] <file> [<file> ...]
"""

import argparse
import re
import sys
from collections import defaultdict
from pathlib import Path

# ── Terminal colors ────────────────────────────────────────────────────────────
BOLD = "\033[1m"
DIM = "\033[2m"
CYAN = "\033[36m"
GREEN = "\033[32m"
YELLOW = "\033[33m"
RED = "\033[31m"
RESET = "\033[0m"

NO_COLOR = not sys.stdout.isatty()
if NO_COLOR:
    BOLD = DIM = CYAN = GREEN = YELLOW = RED = RESET = ""


# ── Parsers ────────────────────────────────────────────────────────────────────
def parse_collapsed_stacks(path: str) -> list[tuple[list[str], int]]:
    """Parse collapsed stack format: 'func1;func2;func3 count'."""
    stacks = []
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = line.rsplit(" ", 1)
            if len(parts) != 2:
                continue
            stack_str, count_str = parts
            try:
                count = int(count_str)
            except ValueError:
                continue
            stacks.append((stack_str.split(";"), count))
    return stacks


def parse_perf_script(path: str) -> list[tuple[list[str], int]]:
    """Parse `perf script` output into collapsed stacks.

    perf script format (each sample):
        command  pid [cpu] timestamp: event:
                address symbol+offset (module)
                address symbol+offset (module)
                ...
        <blank line>
    """
    stacks = []
    current_frames: list[str] = []

    frame_re = re.compile(
        r"^\s+[0-9a-f]+\s+"  # leading whitespace + hex address
        r"(.+?)"  # symbol (captured)
        r"(?:\+0x[0-9a-f]+)?"  # optional +offset
        r"\s+\("  # space + opening paren for module
    )

    with open(path) as f:
        for line in f:
            line = line.rstrip()

            if not line:
                # Blank line → end of sample
                if current_frames:
                    current_frames.reverse()  # perf gives leaf-first
                    stacks.append((current_frames, 1))
                    current_frames = []
                continue

            m = frame_re.match(line)
            if m:
                sym = m.group(1).strip()
                # Skip unknown / hex-only / kernel symbols
                if sym.startswith("[") or re.match(r"^0x[0-9a-f]+$", sym):
                    continue
                current_frames.append(sym)

    # Flush last sample
    if current_frames:
        current_frames.reverse()
        stacks.append((current_frames, 1))

    return stacks


# ── Analysis ───────────────────────────────────────────────────────────────────
def analyze(
    stacks: list[tuple[list[str], int]],
) -> tuple[dict[str, int], dict[str, int], dict[str, dict[str, int]], int]:
    """Compute self time, total time, and caller->callee maps."""
    self_time: dict[str, int] = defaultdict(int)
    total_time: dict[str, int] = defaultdict(int)
    callee_time: dict[str, dict[str, int]] = defaultdict(lambda: defaultdict(int))
    total_samples = 0

    for frames, count in stacks:
        total_samples += count
        if not frames:
            continue

        # Self time: leaf function only
        self_time[frames[-1]] += count

        # Total time: every unique function in the stack
        seen: set[str] = set()
        for i, frame in enumerate(frames):
            if frame not in seen:
                total_time[frame] += count
                seen.add(frame)
            # Caller -> callee relationship
            if i + 1 < len(frames):
                callee_time[frame][frames[i + 1]] += count

    return self_time, total_time, callee_time, total_samples


# ── Display ────────────────────────────────────────────────────────────────────
def clean_name(name: str) -> str:
    """Shorten verbose Rust/Python symbol names for display."""
    # Python perf trampolines: py::module.path::function → module.path::function
    name = re.sub(r"^py::", "", name)

    # Remove Rust impl wrappers: <Foo as Bar>::method → method
    name = re.sub(r"<.*? as .*?>::", "", name)

    # Shorten common crate prefixes
    for long, short in [
        ("hexz_core::", "core::"),
        ("hexz_cli::", "cli::"),
        ("hexz_loader::", "loader::"),
        ("std::sys::", "std::"),
        ("core::ops::function::", ""),
    ]:
        name = name.replace(long, short)

    # Drop Rust hash suffixes (::h1a2b3c4d5e6f7g8)
    name = re.sub(r"::h[0-9a-f]{16}$", "", name)

    if len(name) > 68:
        name = name[:65] + "..."
    return name


def report(
    title: str,
    stacks: list[tuple[list[str], int]],
    top_n: int = 10,
    threshold: float = 5.0,
) -> str:
    """Format a clean terminal report with drill-down."""
    self_time, total_time, callee_time, total = analyze(stacks)
    if total == 0:
        return f"\n  {title}: no samples collected\n"

    ranked = sorted(self_time.items(), key=lambda kv: -kv[1])[:top_n]
    bar = "\u2550" * 78

    lines: list[str] = []
    lines.append(f"\n{BOLD}{bar}{RESET}")
    lines.append(
        f"{BOLD}  {title} \u2014 Top {top_n} Hotspots  ({total:,} samples){RESET}"
    )
    lines.append(f"{BOLD}{bar}{RESET}\n")

    lines.append(f" {BOLD}{'#':>3}  {'Self%':>6}  {'Total%':>7}  Function{RESET}")
    lines.append(f" {'\u2500' * 3}  {'\u2500' * 6}  {'\u2500' * 7}  {'\u2500' * 58}")

    for rank, (func, self_cnt) in enumerate(ranked, 1):
        self_pct = 100.0 * self_cnt / total
        total_pct = 100.0 * total_time[func] / total
        display = clean_name(func)

        # Color-code by impact
        if self_pct >= 10:
            color = YELLOW
        elif self_pct >= threshold:
            color = CYAN
        else:
            color = ""
        end = RESET if color else ""

        lines.append(
            f" {rank:>3}  {color}{self_pct:>5.1f}%{end}  {total_pct:>6.1f}%  {display}"
        )

        # Drill-down: show hot callees for functions above threshold
        if self_pct >= threshold and func in callee_time:
            callees = sorted(callee_time[func].items(), key=lambda kv: -kv[1])
            shown = False
            for callee, ccnt in callees:
                cpct = 100.0 * ccnt / total
                if cpct >= threshold:
                    cdisplay = clean_name(callee)
                    lines.append(
                        f" {'':>3}  {'':>6}  {'':>7}"
                        f"  {DIM}\u2514\u2500 {cpct:>5.1f}%  {cdisplay}{RESET}"
                    )
                    shown = True
            if shown:
                lines.append("")

    lines.append("")
    return "\n".join(lines)


# ── Main ───────────────────────────────────────────────────────────────────────
def main():
    ap = argparse.ArgumentParser(description="Analyze profiling data.")
    ap.add_argument("files", nargs="+", help="Profile files (.stacks or .perf)")
    ap.add_argument("--top", type=int, default=10, help="Top N functions (default: 10)")
    ap.add_argument(
        "--threshold",
        type=float,
        default=5.0,
        help="Drill-down threshold %% (default: 5.0)",
    )
    args = ap.parse_args()

    for path in args.files:
        p = Path(path)
        if not p.exists():
            print(f"  {DIM}skipping {path} (not found){RESET}")
            continue

        if p.suffix == ".stacks":
            stacks = parse_collapsed_stacks(path)
        elif p.suffix == ".perf":
            stacks = parse_perf_script(path)
        else:
            print(
                f"  {DIM}skipping {path} (unknown format, use .stacks or .perf){RESET}"
            )
            continue

        title = p.stem.replace("_", " ").title()
        print(report(title, stacks, top_n=args.top, threshold=args.threshold))


if __name__ == "__main__":
    main()
