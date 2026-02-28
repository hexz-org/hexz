"""Example: Cloud-Native S3 Streaming.

This example demonstrates how to use Hexz to stream data directly from
Amazon S3 without downloading the entire snapshot first. This is
ideal for exploratory data analysis on massive remote datasets.

NOTE: This script requires valid AWS credentials or a custom S3
endpoint (like MinIO) to run for real.
"""

import hexz


def run_example():
    # Example S3 URI
    # In a real scenario, this would be a multi-terabyte dataset
    s3_uri = "s3://my-bucket/datasets/imagenet_full.hxz"

    print(f"Streaming from: {s3_uri}")
    print("Note: This is a code template and will fail without real S3 access.")

    try:
        # 1. Open a remote snapshot
        # Hexz handles the S3 authentication and range-requests internally.
        # We specify a large cache because network latency is high.
        with hexz.Reader(
            s3_uri,
            cache_size="1GB",
            s3_region="us-east-1",
            # endpoint_url="http://localhost:9000"  # For MinIO/LocalStack
        ) as reader:
            # 2. Get Metadata (only reads the header/index from S3)
            meta = reader.metadata
            print(f"Snapshot Version: {meta.version}")
            print(f"Total Size: {meta.primary_size / (1024**3):.2f} GB")

            # 3. Pull a small subset
            # Imagine we just want to see the first 1KB of the dataset
            print("Fetching first 1KB...")
            header_sample = reader.read(1024, offset=0)
            print(f"Read {len(header_sample)} bytes from header.")
            # This uses an S3 Range Request - very efficient!
            print("Fetching random 1MB chunk from the middle...")
            middle_offset = meta.primary_size // 2
            chunk = reader.read(1024 * 1024, offset=middle_offset)

            print(f"Successfully read {len(chunk)} bytes from S3.")

    except Exception as e:
        print(f"\nCould not connect to S3 (expected): {e}")
        print("\nTo run this for real:")
        print("1. Install aws-cli and run 'aws configure'")
        print("2. Replace 's3_uri' with a path to your own .hxz file")
        print("3. Ensure your IAM policy allows s3:GetObject on the bucket")


if __name__ == "__main__":
    run_example()
