"""Cryptographic operations for Hexz snapshots.

This module provides key generation, signing, and verification functions
for securing Hexz snapshots with Ed25519 signatures.
"""

from typing import Optional
from . import hexz_loader
from .typing import PathLike


def keygen(private_key: PathLike, public_key: PathLike) -> None:
    """Generate a new Ed25519 keypair for signing snapshots.

    Args:
        private_key: Path where private key will be written
        public_key: Path where public key will be written

    Example:
        >>> from hexz import crypto
        >>> crypto.keygen("snapshot.key", "snapshot.pub")
        >>> # Remember to set restrictive permissions on the private key
    """
    import shutil
    import tempfile

    # Create a temporary directory for key generation
    with tempfile.TemporaryDirectory() as temp_dir:
        # Generate keypair in temp directory
        priv_path, pub_path = hexz_loader.keygen(temp_dir)

        # Move generated keys to desired locations
        shutil.move(priv_path, str(private_key))
        shutil.move(pub_path, str(public_key))


def sign(snapshot: PathLike, private_key: PathLike) -> None:
    """Sign a snapshot with a private key.

    Args:
        snapshot: Path to .st snapshot file
        private_key: Path to private key file

    Example:
        >>> from hexz import crypto
        >>> crypto.sign("snapshot.hxz", "snapshot.key")
    """
    hexz_loader.sign_image(str(snapshot), str(private_key))


def verify(
    snapshot: PathLike,
    public_key: PathLike,
    signature: Optional[PathLike] = None,
) -> bool:
    """Verify a snapshot signature with a public key.

    Args:
        snapshot: Path to .st snapshot file
        public_key: Path to public key file
        signature: Optional path to signature file (if separate)

    Returns:
        True if signature is valid, False otherwise

    Example:
        >>> from hexz import crypto
        >>> if crypto.verify("snapshot.hxz", "snapshot.pub"):
        ...     print("Signature valid!")
        ... else:
        ...     print("Signature verification failed!")
    """
    try:
        if signature:
            hexz_loader.verify_image(str(snapshot), str(public_key), str(signature))
        else:
            hexz_loader.verify_image(str(snapshot), str(public_key))
        # verify_image returns None on success, raises on failure
        return True
    except (ValueError, RuntimeError):
        return False


__all__ = ["keygen", "sign", "verify"]
