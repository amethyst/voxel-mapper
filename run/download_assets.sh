#!/bin/bash
set -e

ARRAY_MATERIALS_FILE_ID=1FIBbm26bb4Y2S57wZQMETDmYq30aLD6M

echo "Downloading array_materials..."
python3 run/google_drive.py $ARRAY_MATERIALS_FILE_ID array_materials.zip
unzip array_materials.zip -d assets
rm array_materials.zip
echo "Done"
