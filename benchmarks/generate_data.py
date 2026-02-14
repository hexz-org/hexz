#!/usr/bin/env python3
"""CLI script to generate test data."""

import sys
from pathlib import Path

from benchmarks.utils.data_generator import main

# Add src to path
benchmarks_root = Path(__file__).parent
sys.path.insert(0, str(benchmarks_root / "src"))


if __name__ == "__main__":
    # Set default output dir to benchmarks/data/ if not explicitly specified
    if "--output-dir" not in sys.argv:
        sys.argv.extend(["--output-dir", str(benchmarks_root / "data")])
    main()
