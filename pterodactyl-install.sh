#!/bin/bash

echo -e "Installing dependencies..."
apt update
apt install -y libc-bin libc6 unzip

echo -e "Downloading latest release..."
curl -L --output latest.zip "https://github.com/LassiKuisma/lethal-company-mod-tinder/releases/download/latest/latest.zip"
unzip -o latest.zip -d /mnt/server

echo -e "Install complete!"
