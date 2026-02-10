Reader and AsyncReader
======================

.. autoclass:: strata.Reader
   :members:
   :noindex:

.. autoclass:: strata.AsyncReader
   :members:
   :noindex:

Example: sequential and random access
-------------------------------------

.. code-block:: python

   import strata

   with strata.Reader("dataset.st") as reader:
       # Sequential read
       chunk = reader.read(4096)
       # Random access
       block = reader.read_at(offset=10000, size=1024)
       # Slice notation
       block = reader[10000:11024]
       # Iterate in 1MB chunks (zero-copy)
       for buf in reader.iter_chunks(chunk_size=1024 * 1024):
           process(buf)
       # Metadata
       print(reader.metadata.compression, reader.size)
