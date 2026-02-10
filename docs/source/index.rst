Strata Python API
=================

Strata is a high-performance snapshot storage library for machine learning and VM workloads.
This is the **Python API reference**.

Quick links
-----------

* For a 5-minute quick start, see the repository ``docs/quickstart.md``.
* :ref:`api-reference` — Full API reference (below).

Quick example
-------------

.. code-block:: python

   import strata

   # Create a snapshot
   with strata.open("dataset.st", mode="w", compression="lz4") as writer:
       writer.add("data/")

   # Read with random access
   with strata.open("dataset.st") as reader:
       data = reader[0:4096]
       meta = reader.metadata

   # ML training
   dataset = strata.Dataset("dataset.st", item_size=1024)
   loader = torch.utils.data.DataLoader(dataset, batch_size=32)

.. _api-reference:

API Reference
-------------

.. toctree::
   :maxdepth: 2
   :caption: API Reference

   api/index

Indices and tables
------------------

* :genindex:
* :modindex:
* :search:
