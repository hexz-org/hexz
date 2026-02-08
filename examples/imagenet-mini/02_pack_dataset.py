# examples/imagenet-mini/02_pack_dataset.py
# ⚠️ TARGET API

import strata

print("Packing dataset...")

# This calls the Rust function directly.
# No need for subprocess.run() or bash scripts.
strata.pack(
    input_dir="./raw_data",
    output_file="./imagenet-mini.st",
    compression="lz4",
    deduplication=True,
    threads=8,
)

print("Done! .st file created.")
