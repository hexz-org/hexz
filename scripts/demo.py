import os
import shutil

import hexz


def main():
    print(f"Hexz Python API Demo (v{hexz.version()})")

    # 1. Prepare dummy data
    data_dir = "demo_data"
    os.makedirs(data_dir, exist_ok=True)

    file1_path = os.path.join(data_dir, "hello.txt")
    with open(file1_path, "w") as f:
        f.write("Hello from Hexz Python API!\n" * 100)

    file2_path = os.path.join(data_dir, "random.bin")
    with open(file2_path, "wb") as f:
        f.write(os.urandom(1024 * 1024))  # 1MB of random data

    print(f"Created dummy data in '{data_dir}'")

    # 2. Pack data into an archive
    archive_path = "demo.hxz"
    print(f"Packing '{data_dir}' into '{archive_path}'...")
    hexz.pack(data_dir, archive_path, compression="zstd")

    # 3. Open and Inspect Archive
    print(f"\nOpening archive '{archive_path}'...")
    with hexz.Archive(archive_path) as arch:
        files = arch.namelist()
        print(f"Files in archive: {files}")

        # 4. Read entire file
        print(f"\nReading 'hello.txt' fully...")
        content = arch.read("hello.txt")
        print(f"Content start: {content[:30].decode()}...")
        print(f"Total size: {len(content)} bytes")

        # 5. Stream and Random Access
        print(f"\nStreaming 'random.bin' with random access...")
        with arch.open("random.bin") as f:
            # Read first 10 bytes
            head = f.read(10)
            print(f"First 10 bytes: {head.hex()}")

            # Seek to near the end
            f.seek(-10, 2)  # 2 is os.SEEK_END
            tail = f.read(10)
            print(f"Last 10 bytes:  {tail.hex()}")
            print(f"Current position (tell): {f.tell()}")

    # Cleanup
    print("\nCleaning up demo files...")
    os.remove(archive_path)
    shutil.rmtree(data_dir)
    print("Done!")


if __name__ == "__main__":
    main()
