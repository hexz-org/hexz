Utilities: inspect, analyze, diff, verify, info
================================================

.. autoclass:: strata.Metadata
   :members:
   :noindex:

.. autofunction:: strata.inspect
   :noindex:

.. autoclass:: strata.AnalysisReport
   :noindex:

.. autofunction:: strata.analyze
   :noindex:

.. autofunction:: strata.diff
   :noindex:

.. autofunction:: strata.verify
   :noindex:

.. autofunction:: strata.info
   :noindex:

.. autofunction:: strata.merge_overlay
   :noindex:

Example: inspect and verify
---------------------------

.. code-block:: python

   import strata

   meta = strata.inspect("snapshot.st")
   print(meta.version, meta.compression, meta.disk_size, meta.num_blocks)

   ok = strata.verify("snapshot.st", checksum=True, structure=True)

   report = strata.analyze("data.img")
   print(f"Dedup savings: {report.savings_percent:.1f}%")

   diff_info = strata.diff("base.st", "updated.st")
