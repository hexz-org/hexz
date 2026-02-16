import time
import random
import sys
import os

FILE_SIZE = 100 * 1024 * 1024  # 100 MB
OUTPUT_DIR = "data"
FILENAME = os.path.join(OUTPUT_DIR, "complex_logs.json")

USER_AGENTS = [
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64)",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
    "Mozilla/5.0 (X11; Linux x86_64; rv:100.0)",
    "curl/7.68.0",
    "PostmanRuntime/7.29.0",
]

ENDPOINTS = [
    "/api/v1/users",
    "/api/v1/auth/login",
    "/health",
    "/static/css/main.css",
    "/api/v1/orders/create",
]

STATUSES = [200, 201, 400, 401, 403, 404, 500]


def generate():
    # Create data directory if it doesn't exist
    if not os.path.exists(OUTPUT_DIR):
        os.makedirs(OUTPUT_DIR)
        print(f"Created directory: {OUTPUT_DIR}")

    print(
        f"Generating {FILE_SIZE / 1024 / 1024:.2f} MB of complex JSON log data into {FILENAME}..."
    )

    start_time = time.time()
    current_size = 0

    with open(FILENAME, "w", encoding="utf-8") as f:
        buffer = []
        buffer_size = 0

        while current_size < FILE_SIZE:
            ts = int(time.time() * 1000)
            req_id = random.getrandbits(64)
            ua = random.choice(USER_AGENTS)
            path = random.choice(ENDPOINTS)
            status = random.choice(STATUSES)
            latency = random.randint(5, 500)

            line = (
                f'{{"ts":{ts},"level":"INFO","req_id":"{req_id:x}",'
                f'"ua":"{ua}","path":"{path}","status":{status},'
                f'"latency_ms":{latency}}}\n'
            )

            buffer.append(line)
            line_len = len(line)
            buffer_size += line_len
            current_size += line_len

            if buffer_size > 65536:
                f.writelines(buffer)
                buffer = []
                buffer_size = 0
                sys.stdout.write(f"\rProgress: {current_size / 1024 / 1024:.2f} MB")
                sys.stdout.flush()

        if buffer:
            f.writelines(buffer)

    print(f"\nDone! Created '{FILENAME}' in {time.time() - start_time:.2f} seconds.")


if __name__ == "__main__":
    generate()
