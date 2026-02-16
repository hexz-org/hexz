import sys
import os
import random


def generate_mixed_data(filename, size_gb):
    size_bytes = int(float(size_gb) * 1024 * 1024 * 1024)
    chunk_size = 1024 * 1024  # 1 MB

    print(f"Generating {size_gb} GB of mixed data into {filename}...")

    with open(filename, "wb") as f:
        bytes_written = 0
        while bytes_written < size_bytes:
            # Generate mix of random data (hard to compress) and repeated data (easy to compress)
            if random.random() < 0.5:
                # Random data
                data = os.urandom(chunk_size)
            else:
                # Repeated data
                data = b"\x00" * chunk_size

            write_len = min(chunk_size, size_bytes - bytes_written)
            f.write(data[:write_len])
            bytes_written += write_len

            sys.stdout.write(f"\rProgress: {bytes_written / 1024 / 1024:.0f} MB")
            sys.stdout.flush()

    print("\nDone.")


if __name__ == "__main__":
    if len(sys.argv) < 3:
        print("Usage: python3 generate_mixed_data.py <filename> <size_gb>")
        sys.exit(1)

    generate_mixed_data(sys.argv[1], sys.argv[2])
