Exceptions
==========

.. autoexception:: strata.StrataError
   :noindex:

.. autoexception:: strata.IOError
   :noindex:

.. autoexception:: strata.NetworkError
   :noindex:

.. autoexception:: strata.FormatError
   :noindex:

.. autoexception:: strata.ValidationError
   :noindex:

.. autoexception:: strata.CompressionError
   :noindex:

.. autoexception:: strata.EncryptionError
   :noindex:

.. autoexception:: strata.MountError
   :noindex:

.. autoexception:: strata.CacheError
   :noindex:

.. autoexception:: strata.VersionError
   :noindex:

Example: handling errors
------------------------

.. code-block:: python

   import strata

   try:
       with strata.open("missing.st") as r:
           r.read(1024)
   except strata.IOError as e:
       print("I/O failed:", e)
   except strata.FormatError as e:
       print("Invalid format:", e)
   except strata.StrataError as e:
       print("Strata error:", e)
