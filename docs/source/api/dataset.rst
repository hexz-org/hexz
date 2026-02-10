Dataset and TFDataset
=====================

.. autoclass:: strata.Dataset
   :members:
   :noindex:

.. autoclass:: strata.TFDataset
   :members:
   :noindex:

Example: PyTorch DataLoader
---------------------------

.. code-block:: python

   import torch
   import strata

   dataset = strata.Dataset(
       "imagenet.st",
       item_size=150528,  # 224*224*3
       transform=torchvision.transforms.ToTensor(),
       cache_size_mb=512,
       shuffle=True,
       seed=42,
   )
   loader = torch.utils.data.DataLoader(dataset, batch_size=32, num_workers=4)
   for batch in loader:
       train_step(batch)
