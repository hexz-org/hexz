#!/usr/bin/env python3
import os
import argparse
import shutil
from pathlib import Path
import numpy as np
import hexz


def create_blob(size_mb, seed=None):
    if seed is not None:
        np.random.seed(seed)
    return np.random.bytes(size_mb * 1024 * 1024)


def format_size(size_bytes):
    return f"{size_bytes / 1024 / 1024:.2f} MB"


class DedupDeepDive:
    def __init__(self, work_dir, size_mb, no_clean):
        self.work_dir = Path(work_dir)
        self.size_mb = size_mb
        self.no_clean = no_clean
        if self.work_dir.exists():
            shutil.rmtree(self.work_dir)
        self.work_dir.mkdir(parents=True)

    def run_all(self):
        print(
            f"Starting Global Deduplication Deep Dive | Workspace: {self.work_dir} | Base unit: {self.size_mb} MB"
        )
        try:
            self.linear_chain()
            self.branching_versions()
            self.global_shards()
            self.shift_resilience()
        finally:
            if not self.no_clean:
                shutil.rmtree(self.work_dir)
            else:
                print(f"Files preserved in: {self.work_dir.absolute()}")

    def linear_chain(self):
        v1_path, v2_path, v3_path = (
            self.work_dir / "l_v1.hxz",
            self.work_dir / "l_v2.hxz",
            self.work_dir / "l_v3.hxz",
        )
        data_v1 = create_blob(self.size_mb, seed=100)
        with hexz.Writer(v1_path) as w:
            w.add_bytes(data_v1)
        split = int(len(data_v1) * 0.9)
        data_v2 = data_v1[:split] + create_blob(self.size_mb // 10, seed=101)
        with hexz.Writer(v2_path, parent=v1_path) as w:
            w.add_bytes(data_v2)
        split = int(len(data_v2) * 0.9)
        data_v3 = data_v2[:split] + create_blob(self.size_mb // 10, seed=102)
        with hexz.Writer(v3_path, parent=v2_path) as w:
            w.add_bytes(data_v3)
        self.print_stats(
            "Linear Chain",
            [v1_path, v2_path, v3_path],
            [len(data_v1), len(data_v2), len(data_v3)],
        )

    def branching_versions(self):
        base_p, ta_p, tb_p = (
            self.work_dir / "b_base.hxz",
            self.work_dir / "b_ta.hxz",
            self.work_dir / "b_tb.hxz",
        )
        base_data = create_blob(self.size_mb, seed=200)
        with hexz.Writer(base_p) as w:
            w.add_bytes(base_data)
        ta_data = base_data + create_blob(5, seed=201)
        with hexz.Writer(ta_p, parent=base_p) as w:
            w.add_bytes(ta_data)
        tb_data = base_data + create_blob(5, seed=202)
        with hexz.Writer(tb_p, parent=base_p) as w:
            w.add_bytes(tb_data)
        self.print_stats(
            "Branching",
            [base_p, ta_p, tb_p],
            [len(base_data), len(ta_data), len(tb_data)],
        )

    def global_shards(self):
        s1_p, s2_p, c_p = (
            self.work_dir / "shard1.hxz",
            self.work_dir / "shard2.hxz",
            self.work_dir / "comb.hxz",
        )
        s1_d, s2_d = (
            create_blob(self.size_mb, seed=300),
            create_blob(self.size_mb, seed=301),
        )
        with hexz.Writer(s1_p) as w:
            w.add_bytes(s1_d)
        with hexz.Writer(s2_p) as w:
            w.add_bytes(s2_d)
        c_d = s1_d[: len(s1_d) // 2] + create_blob(2, seed=302) + s2_d[: len(s2_d) // 2]
        with hexz.Writer(c_p, parent=[s1_p, s2_p]) as w:
            w.add_bytes(c_d)
        self.print_stats(
            "Global Shards", [s1_p, s2_p, c_p], [len(s1_d), len(s2_d), len(c_d)]
        )

    def shift_resilience(self):
        o_p, s_p = self.work_dir / "orig.hxz", self.work_dir / "shift.hxz"
        o_d = create_blob(self.size_mb, seed=400)
        with hexz.Writer(o_p) as w:
            w.add_bytes(o_d)
        s_d = b"X" * 1024 + o_d
        with hexz.Writer(s_p, parent=o_p) as w:
            w.add_bytes(s_d)
        self.print_stats("Shift Resilience", [o_p, s_p], [len(o_d), len(s_d)])

    def print_stats(self, title, paths, logical_sizes):
        t_log, t_phys = sum(logical_sizes), sum(os.path.getsize(p) for p in paths)
        savings = (1 - (t_phys / t_log)) * 100
        print(
            f"{title:18} | Logical: {format_size(t_log):>10} | Physical: {format_size(t_phys):>10} | Savings: {savings:5.1f}%"
        )


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--size-mb", type=int, default=100)
    parser.add_argument("--no-clean", action="store_true")
    parser.add_argument("--dir", type=str, default="dedup_test")
    args = parser.parse_args()
    DedupDeepDive(args.dir, args.size_mb, args.no_clean).run_all()
