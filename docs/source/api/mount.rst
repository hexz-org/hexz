Mount and unmount
=================

.. autofunction:: strata.mount
   :noindex:

.. autofunction:: strata.unmount
   :noindex:

.. autoclass:: strata.MountPoint
   :members:
   :noindex:

Example: mount snapshot as filesystem
-------------------------------------

.. code-block:: python

   import os
   import strata

   with strata.mount("snapshot.st") as mp:
       # mp.path is the mount point (e.g. /tmp/...)
       for name in os.listdir(mp.path):
           print(name)  # e.g. disk, memory
