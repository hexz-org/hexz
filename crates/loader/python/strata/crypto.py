"""Cryptographic operations for Strata snapshots.

This module provides key generation, signing, and verification functions
for securing Strata snapshots with Ed25519 signatures.
"""

from typing import Optional
from . import strata_loader
from .typing import PathLike


def keygen(private_key: PathLike, public_key: PathLike) -> None:
    """Generate a new Ed25519 keypair for signing snapshots.

    Args:
        private_key: Path where private key will be written
        public_key: Path where public key will be written

    Example:
        >>> from strata import crypto
        >>> crypto.keygen("snapshot.key", "snapshot.pub")
        >>> # Remember to set restrictive permissions on the private key
    """
    strata_loader.keygen(str(private_key), str(public_key))


def sign(snapshot: PathLike, private_key: PathLike) -> None:
    """Sign a snapshot with a private key.

    Args:
        snapshot: Path to .st snapshot file
        private_key: Path to private key file

    Example:
        >>> from strata import crypto
        >>> crypto.sign("snapshot.st", "snapshot.key")
    """
    strata_loader.sign_image(str(snapshot), str(private_key))


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
        >>> from strata import crypto
        >>> if crypto.verify("snapshot.st", "snapshot.pub"):
        ...     print("Signature valid!")
        ... else:
        ...     print("Signature verification failed!")
    """
    if signature:
        return strata_loader.verify_image(
            str(snapshot), str(public_key), str(signature)
        )
    else:
        return strata_loader.verify_image(str(snapshot), str(public_key))


__all__ = ["keygen", "sign", "verify"]
