"""Example: Secure Model Distribution (Signing).

This example demonstrates how to sign a Hexz snapshot with a private key
and verify its integrity with a public key before loading. This is
critical for secure model distribution in production.
"""

import hexz
import os

_DATA_DIR = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))), ".data", "storage"
)


def run_example():
    if not hasattr(hexz, "crypto") or hexz.crypto is None:
        print("Cryptographic signing feature is not enabled in this build.")
        return

    os.makedirs(_DATA_DIR, exist_ok=True)

    snapshot_path = os.path.join(_DATA_DIR, "secure_model.hxz")
    priv_key = os.path.join(_DATA_DIR, "admin_private.key")
    pub_key = os.path.join(_DATA_DIR, "admin_public.key")

    # 1. Create a snapshot
    print("Packing model...")
    with hexz.Writer(snapshot_path) as writer:
        writer.add(b"CRITICAL_MODEL_WEIGHTS_DO_NOT_TAMPER")

    # 2. Generate a keypair
    print("Generating cryptographic keypair...")
    hexz.crypto.keygen(priv_key, pub_key)

    # 3. Sign the snapshot
    print("Signing snapshot...")
    hexz.crypto.sign(snapshot_path, priv_key)

    # 4. Verification (Success)
    print("\nVerifying signature with public key...")
    is_valid = hexz.verify(snapshot_path, public_key=pub_key)
    print(f"Is signature valid? {is_valid}")
    assert is_valid

    # 5. Tampering (Simulate an attack)
    print("\nSimulating tampering (modifying one byte)...")
    with open(snapshot_path, "r+b") as f:
        f.seek(4096)
        f.write(b"\xff")

    is_valid_after_tamper = hexz.verify(snapshot_path, public_key=pub_key)
    print(f"Is signature valid after tampering? {is_valid_after_tamper}")
    assert not is_valid_after_tamper
    print("✓ Secure loader successfully rejected the tampered model.")

    # Clean up
    for p in [snapshot_path, priv_key, pub_key, snapshot_path + ".sig"]:
        if os.path.exists(p):
            os.remove(p)


if __name__ == "__main__":
    run_example()
