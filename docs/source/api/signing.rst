Signing and verification (keygen, sign_image, verify_image)
============================================================

.. autofunction:: strata.keygen
   :noindex:

.. autofunction:: strata.sign_image
   :noindex:

.. autofunction:: strata.verify_image
   :noindex:

Example: sign and verify a snapshot
-----------------------------------

.. code-block:: python

   import strata

   priv_path, pub_path = strata.keygen("/path/to/keys")
   strata.sign_image("release.st", priv_path)
   strata.verify_image("release.st", pub_path)
