# examples/imagenet-mini/01_generate_fake_data.py
import os
import random

import numpy as np
from PIL import Image
from tqdm import tqdm

DATA_DIR = "./raw_data"
NUM_CLASSES = 10
IMGS_PER_CLASS = 500


def create_fake_dataset():
    if os.path.exists(DATA_DIR):
        print(f"Directory {DATA_DIR} already exists. Skipping generation.")
        return

    print(f"Generating {NUM_CLASSES * IMGS_PER_CLASS} fake images...")
    os.makedirs(DATA_DIR, exist_ok=True)

    for class_id in range(NUM_CLASSES):
        class_dir = os.path.join(DATA_DIR, f"class_{class_id}")
        os.makedirs(class_dir, exist_ok=True)

        for img_id in range(IMGS_PER_CLASS):
            # Create a 64x64 random noise image
            img_array = np.random.randint(0, 255, (64, 64, 3), dtype=np.uint8)
            img = Image.fromarray(img_array)

            # Save it
            img.save(os.path.join(class_dir, f"img_{img_id}.jpg"), quality=85)

    print("Done! Raw data is in ./raw_data")


if __name__ == "__main__":
    create_fake_dataset()
