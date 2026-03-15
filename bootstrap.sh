#!/bin/bash
# LiveRAID Universal Bootstrap Script
# Designed to be executed via: curl -sL <URL> | sudo bash

set -e

echo "Starting LiveRAID Configurator Bootstrap..."

# Ensure the script is run as root
if [ "$EUID" -ne 0 ]; then
  echo "Please run as root (or use sudo)."
  exit 1
fi

# Determine the package manager
if [ -x "$(command -v apt-get)" ]; then
    echo "Detected Advanced Package Tool (APT). Updating..."
    apt-get update -yq
    echo "Installing requirements..."
    export DEBIAN_FRONTEND=noninteractive
    apt-get install -yq python3-gi python3-gi-cairo gir1.2-gtk-3.0 mdadm parted e2fsprogs btrfs-progs dosfstools xfsprogs curl wget
elif [ -x "$(command -v dnf)" ]; then
    echo "Detected DNF (Fedora/RHEL). Installing requirements..."
    dnf install -y python3-gobject gtk3 mdadm parted e2fsprogs btrfs-progs dosfstools xfsprogs curl wget
elif [ -x "$(command -v pacman)" ]; then
    echo "Detected Pacman (Arch). Installing requirements..."
    pacman -Sy --noconfirm python-gobject gtk3 mdadm parted e2fsprogs btrfs-progs dosfstools xfsprogs curl wget
elif [ -x "$(command -v zypper)" ]; then
    echo "Detected Zypper (SUSE). Installing requirements..."
    zypper install -y python3-gobject gtk3 mdadm parted e2fsprogs btrfs-progs dosfstools xfsprogs curl wget
else
    echo "Could not detect a supported package manager (apt, dnf, pacman, zypper)."
    echo "Please install dependencies manually: python3-gobject, gtk3, mdadm, parted, and mkfs tools."
    exit 1
fi

echo "Dependencies installed."

# Define a working directory for pulling the python scripts
LIVERAID_DIR="/tmp/liveraid"
mkdir -p "$LIVERAID_DIR"

# Since this isn't on GitHub yet, for local execution we will just use the files from the Desktop Vibe_Projects folder
# We simulate the download by copying from the project directory if it exists, otherwise we'd use wget.
PROJECT_SRC="/home/user/Desktop/Vibe_Projects/LiveRAID"

if [ -d "$PROJECT_SRC" ]; then
    echo "Copying source from $PROJECT_SRC to $LIVERAID_DIR for execution..."
    cp "$PROJECT_SRC"/main.py "$LIVERAID_DIR/"
    cp "$PROJECT_SRC"/backend.py "$LIVERAID_DIR/"
    cp "$PROJECT_SRC"/ui.py "$LIVERAID_DIR/"
else
    echo "This is where we would download the scripts from GitHub..."
    # Example: wget https://raw.githubusercontent.com/user/liveraid/main/main.py -O $LIVERAID_DIR/main.py
    # Example: wget https://raw.githubusercontent.com/user/liveraid/main/backend.py -O $LIVERAID_DIR/backend.py
    # Example: wget https://raw.githubusercontent.com/user/liveraid/main/ui.py -O $LIVERAID_DIR/ui.py
    # But since we are generating them locally, we stop if we can't find them.
    echo "Source files not found! Ensure main.py, backend.py, and ui.py exist in $PROJECT_SRC"
    exit 1
fi

chmod +x "$LIVERAID_DIR"/main.py

echo "Starting LiveRAID Configurator GUI..."
cd "$LIVERAID_DIR"
# Execute as the user executing sudo, but we need root for disks, so we will just run python3 directly as root
# Note: running graphical apps as root can have xhost issues on Wayland, so we allow local connections:
xhost +local: || true
python3 main.py

echo "LiveRAID Configurator closed."
