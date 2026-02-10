Array I/O (NumPy)
=================

.. autofunction:: strata.read_array
   :noindex:

.. autofunction:: strata.write_array
   :noindex:

.. autoclass:: strata.ArrayView
   :members:
   :noindex:

Example: read/write arrays
--------------------------

.. code-block:: python

   import numpy as np
   import strata

   # Write array
   data = np.random.rand(1000, 784).astype("float32")
   strata.write_array("data.st", data, compression="lz4")

   # Read array
   arr = strata.read_array("data.st", offset=0, shape=(1000, 784), dtype="float32")

   # Memmap-style view (no full load)
   view = strata.ArrayView("data.st", shape=(10000, 784), dtype="float32")
   batch = view[0:100]  # First 100 rows
