Writer
======

.. autoclass:: strata.Writer
   :members:
   :noindex:

Example: build a snapshot
-------------------------

.. code-block:: python

   import strata

   with strata.Writer("output.st", compression="zstd", packing="balanced") as w:
       w.add("/path/to/disk.img")
       w.add_bytes(b"raw bytes")
       w.add_metadata({"created": "2026-02-09"})
   # Snapshot is finalized on context exit
