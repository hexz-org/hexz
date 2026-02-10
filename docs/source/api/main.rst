strata — Main module
====================

.. automodule:: strata
   :members: open, version
   :noindex:

open()
------

Open a Strata snapshot for reading or writing.

.. code-block:: python

   import strata

   # Read
   with strata.open("data.st") as reader:
       data = reader.read(4096)

   # Write
   with strata.open("out.st", mode="w", compression="lz4") as writer:
       writer.add("input.img")

version()
---------

Return the library version string.
