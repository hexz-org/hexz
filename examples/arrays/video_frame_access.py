"""Example: Efficient Video Frame Access.

This example demonstrates how to use Hexz to store raw video frames and
access specific frames or sequences instantly without decompressing
the entire file.
"""

import numpy as np
import hexz
import time
from pathlib import Path


def run_example():
    # 1. Setup synthetic video data
    # 100 frames, 224x224 RGB (standard for many CV models)
    num_frames = 100
    height, width = 224, 224
    frame_size = height * width * 3

    snapshot_path = "video_dataset.hxz"

    print(f"Creating synthetic video: {num_frames} frames of {height}x{width} RGB")

    # 2. Pack frames into a Hexz snapshot
    # We use the 'ml' profile which is optimized for sequential writes
    # and fast random-access reads.
    with hexz.Writer(snapshot_path, packing="fast") as writer:
        for i in range(num_frames):
            # Create a synthetic frame (e.g., color gradient that changes over time)
            frame = np.full((height, width, 3), i % 256, dtype=np.uint8)
            writer.add(frame.tobytes())

        # Add some metadata about the video
        writer.add_metadata(
            {
                "num_frames": num_frames,
                "height": height,
                "width": width,
                "codec": "raw_rgb8",
            }
        )

    # 3. Random Access: Pulling specific frames
    print(f"\nOpening {snapshot_path} for random access...")
    with hexz.open(snapshot_path) as reader:
        meta = reader.metadata
        print(
            f"Metadata: {meta['num_frames']} frames, {meta['height']}x{meta['width']}"
        )

        # Pull frame #42
        # Offset is frame_index * frame_size
        target_frame = 42
        offset = target_frame * frame_size

        start_time = time.perf_counter()
        frame_data = reader.read(frame_size, offset=offset)
        duration = (time.perf_counter() - start_time) * 1000

        frame_array = np.frombuffer(frame_data, dtype=np.uint8).reshape(
            (height, width, 3)
        )

        print(f"Read frame {target_frame} in {duration:.2f}ms")
        assert np.all(frame_array == target_frame)

    # 4. Slicing: Pulling a "clip" (sequence of frames)
    with hexz.open(snapshot_path) as reader:
        # Pull frames 10 through 15
        start_frame, end_frame = 10, 15
        clip_bytes = reader[start_frame * frame_size : end_frame * frame_size]

        clip = np.frombuffer(clip_bytes, dtype=np.uint8).reshape((-1, height, width, 3))
        print(f"Read clip (frames {start_frame}-{end_frame}): {clip.shape}")

    # Clean up
    Path(snapshot_path).unlink()


if __name__ == "__main__":
    run_example()
