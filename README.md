# LiveRAID Configurator

A LiveCD environment GUI tool designed to easily manage, create, and format Software RAID arrays across any Linux distribution.
Originally written in Rust, LiveRAID has been completely rewritten in **Python** & **GTK3** to eliminate massive dependency overhead and compilation times, allowing it to easily bootstrap itself into any Live Environment (Ubuntu, Fedora, Arch, etc) in seconds.

## Features

- **Universal Bootstrapper**: Supports APT, DNF, Pacman, and Zypper automatically.
- **Dynamic Drive Detection**: Auto-detects unmounted physical drives explicitly available for RAID pairing using `lsblk` and `/proc/mdstat`.
- **Advanced Array Creation**: Supports RAID 0, 1, 5, and 10 with custom Chunk sizes and SSD optimization shortcuts (`--assume-clean`).
- **Comprehensive Formatting**: Generates GPT partition tables and formats software RAIDs immediately with `ext4, btrfs, xfs, zfs, f2fs, exfat, ntfs, vfat`. 
- **Array Destructor**: Can detect active arrays, unmount them, stop them, and safely wipe their underlying physical superblocks so drives can be instantly reused.
- **Hardware Integrations**: Enables TRIM/Discard instructions during formatting, and can flag bootloader partitions.

## How to Install & Run (LiveCD)

In a typical LiveCD Scenario (where you boot from a USB thumb drive), you just need to clone the repo and run the bootstrap script as root.

```bash
git clone https://github.com/freebrew/liveRAID.git
cd liveRAID
sudo bash bootstrap.sh
```

**Wait, what does `bootstrap.sh` do?**
Because LiveCDs reset on every boot, the bootstrap script automatically detects your Linux Distro, connects to its respective package manager, and downloads the missing dependencies needed to build the RAID framework (`mdadm`, `parted`, GUI libraries, and filesystem formatters). After installing the temporary dependencies, it executes the Python application.

## Architecture

* `ui.py` - GTK3 Frontend that handles thread routing and dynamically scans hardware.
* `backend.py` - Interface wrapper interacting natively with `parted`, `mdadm`, `mkfs.*`, and `/proc/mdstat`.
* `main.py` - Application Initializer.
* `bootstrap.sh` - Universal Dependency Manager.

## License

MIT
