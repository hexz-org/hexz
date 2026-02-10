build() and PROFILES
====================

.. autofunction:: strata.build
   :noindex:

PROFILES
--------

.. data:: strata.PROFILES

   Pre-configured build profiles for common use cases. Maps profile names to
   Writer configuration dicts. Use with :func:`strata.build` via the ``profile``
   argument.

   Available profiles:

   - ``ml`` — Machine learning datasets (fast writes, sequential reads)
   - ``eda`` — Exploratory data analysis (balanced)
   - ``embedded`` — Resource-constrained environments (max compression)
   - ``generic`` — General purpose default
   - ``archival`` — Long-term storage (max compression and dedup)

Example: profile-based build
----------------------------

.. code-block:: python

   import strata

   # ML dataset (fast compression, large blocks)
   meta = strata.build("imagenet/", "imagenet.st", profile="ml")

   # Archival (max compression)
   meta = strata.build("backup/", "backup.st", profile="archival")

   # Override profile options
   strata.build("data/", "out.st", profile="generic", block_size=32 * 1024)
