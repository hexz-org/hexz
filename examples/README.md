# Hexz Examples

This directory contains examples demonstrating the key features and use cases of the Hexz Python API.

## Core API
- **`quickstart.py`**: A minimal example of creating and reading a snapshot.
- **`configuration_profiles.py`**: Using build profiles (ML, archival, etc.) to optimize for different workloads.
- **`zero_copy_performance.py`**: Benchmarking Hexz's zero-copy loading against Python's `pickle`.

## Machine Learning & Data Science
- **`resnet_finetune_checkpoints.py`**: Real transfer learning workflow — ResNet-18 on CIFAR-10, three checkpoints (pretrained → head fine-tune → layer4 fine-tune), with measured dedup savings and selective load speedups. Requires `torchvision`.
- **`video_frame_access.py`**: Fast random access to specific frames in a large video dataset.
- **`llm_weight_dedup.py`**: Saving space when storing multiple versions of large model weights.
- **`medical_imaging_3d.py`**: Efficient 2D slicing of massive 3D medical volumes (MRI/CT).
- **`vector_embeddings_lookup.py`**: Using Hexz as a high-performance, read-only vector store.
- **`cloud_s3_streaming.py`**: Streaming data directly from S3 without downloading the entire file.

## Advanced Infrastructure
- **`distributed_loading.py`**: Sharing a single Reader across multiple CPU worker processes (Pickling).
- **`secure_signing.py`**: Cryptographically signing and verifying snapshots for secure distribution.
- **`fuse_mount_explorer.py`**: Mounting snapshots as virtual filesystems (Linux/macOS).
- **`docker_layer_packing.py`**: Packing Docker-style image layers into Hexz snapshots for efficient storage and distribution.

## Advanced Deduplication
- **`global_deduplication.py`**: Demonstrates how Hexz handles redundant data across different files.
- **`incremental_checkpoints.py`**: Using thin snapshots to store only the changes between training steps.
- **`comprehensive_deduplication.py`**: A deep dive into CDC (Content-Defined Chunking) and deduplication ratios.

## Getting Started

1. Ensure you have the library installed:
   ```bash
   # From the project root
   pip install -e .
   ```

2. Run any example:
   ```bash
   python examples/quickstart.py
   ```

Note: Some examples may require additional dependencies like `numpy` or `torch`, and specific features like `fuse` or `signing` must be enabled in the build.
