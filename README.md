# liveRAID (liveRAID)

## Overview

**liveRAID** is a turnkey, cross-distro RAID provisioning tool for Linux, designed for use in live environments. It provides both a modern GUI and a CLI, and fully automates RAID, partitioning, fstab, ESP, and GRUB setup. After running the tool from a live USB, the Linux OS installer will see a ready-to-use, bootable RAID system—no manual steps or Linux expertise required.

- **Supported RAID:** mdadm (software RAID), LVM-RAID
- **Form Factor:** Rust workspace (CLI + GUI frontends)
- **Platforms:** Arch, Debian/Ubuntu, Fedora/RHEL (live media)
- **Zero Knowledge Required:** All disk/bootstrapping is automated

---

## Features

- **End-to-End Automation:**
  - Device discovery → RAID/LVM planning → partition (UEFI/BIOS) → mkfs → mount → fstab → ESP creation/replication → mdadm/lvm config → GRUB install to all ESPs/MBRs → ESP sync → ready for OS install
- **Live Environment Ready:**
  - Designed to run from a live USB session as root
  - Automatically creates and mounts /target, generates /target/etc/fstab, and ensures all ESPs are formatted, mounted, and synced
- **Cross-Distro:**
  - Handles mkinitcpio (Arch), initramfs-tools (Debian/Ubuntu), dracut (Fedora/RHEL)
- **Safety:**
  - Dry-run mode, explicit confirmation for destructive steps, journaling, rollback, and backup of configs
- **GUI + CLI Parity:**
  - Modern GUI (egui/eframe) and CLI (clap-based) with identical logic
- **Comprehensive Filesystem Support:**
  - ext4, ext3, ext2, xfs, btrfs, reiserfs, jfs, ntfs, fat32, exfat
- **No Vendor FakeRAID:**
  - Detects/deactivates/wipes vendor RAID metadata, but does not manage proprietary BIOS RAID

---

## Quick Start

1. **Boot a Linux live USB (as root).**
2. **Clone and run the installer:**
   ```sh
   sudo ./installer.sh --release
   ```
3. **Follow the GUI or CLI to select devices, RAID level, and filesystem.**
4. **Click 'Apply'.**
5. **Done!** The OS installer will see /target as a bootable RAID disk, with fstab, ESP, and GRUB fully configured.

---

## Architecture

### Components
- **raidctl-core:** Rust library (device discovery, planning, execution, journaling)
- **raidctl:** CLI frontend (full parity with GUI)
- **raidctl-gui:** Modern GUI frontend (egui/eframe)

### Data Flow
1. Scan devices
2. Plan RAID/layout
3. Dry-run preview
4. Apply (with udev settle)
5. Artifacts: `/target`, `/etc/fstab`, RAID configs, GRUB, initramfs

### Boot & Initramfs
- **UEFI:** ESP per disk (never on RAID), GRUB with mdraid1x+lvm modules, --removable fallback
- **BIOS:** bios_grub partition for GPT, GRUB install to all MBRs
- **Initramfs:** mkinitcpio/initramfs-tools/dracut, ensures md/LVM modules present

### Disk Layout Templates
- UEFI + mdadm RAID
- UEFI + LVM-RAID
- BIOS + mdadm RAID
- Single-disk (no RAID)

---

## Technical Details

### Device Discovery
- Uses `lsblk -J`, `blkid`, and sysfs
- Filters out mounted/non-disk devices
- Human-readable sizes (TB/GB/MB/KB)

### RAID Level Support
- RAID 0: min 2 disks
- RAID 1: min 2 disks
- RAID 5: min 3 disks
- RAID 6: min 4 disks
- RAID 10: min 4 disks

### Configuration Management
- Default config: `/etc/raidctl/config.toml`
- Supports dry-run, logging, backup, custom mountpoint (default: `/target`)

### Build System
- Rust workspace (2021 edition)
- Shared dependencies: serde, toml, clap, anyhow, thiserror, log, env_logger
- Release builds optimized for production

### Safety
- Prevents using current root device in RAID
- Validates min disk requirements
- Dry-run mode, rollback, backup before changes
- Forbids ESP on RAID

---

## File Structure

```
raidctl/
├── raidctl-core/         # Core logic
├── raidctl-cli/          # CLI frontend
├── raidctl-gui/          # GUI frontend
└── dist/                 # Distribution packages
```

- **installer.sh:** Main install script (installs deps, builds, configures, runs automation)
- **/target:** Mountpoint for OS installer (created/managed automatically)
- **/etc/fstab:** Generated with correct UUIDs and mountpoints
- **/etc/default/grub:** Generated with correct RAID boot params

---

## Dependencies
- **Build tools:** build-essential, pkg-config, Rust toolchain
- **RAID tools:** mdadm, lvm2, e2fsprogs, xfsprogs, grub2, efibootmgr
- **GUI:** libgtk-3-dev, libssl-dev, libudev-dev
- **System:** sudo, curl, wget, policykit-1, ca-certificates

---

## Testing & QA
- Loop device harness for CI
- Matrix: RAID 0/1/5/6/10, mdadm vs LVM
- Distros: Arch, Debian/Ubuntu, Fedora/RHEL
- Acceptance: boot works, rollback safe

---

## Risks & Safeguards
- ESP on RAID → blocked
- Udev race → settle
- Initramfs mismatch → adapters
- User wipe errors → confirmations
- GRUB NVRAM fail → removable
- LVM filter bugs → global_filter

---

## Milestones
- **M1:** Core scan + CLI dry-run
- **M2:** Partition + mdadm path
- **M3:** LVM path
- **M4:** GRUB/initramfs
- **M5:** GUI MVP
- **M6:** Live integration
- **M7:** Docs + release

---

## License

This project is open source under the MIT License.

---

## Contributing

Pull requests and issues are welcome! See the CONTRIBUTING.md for guidelines.

---

## Authors & Credits

- **Core Storage:** mdadm/LVM
- **Boot & Initramfs:** GRUB, initramfs
- **GUI/TUI:** Slint, ratatui
- **Live Integration:** archiso, live-build, livemedia-creator
- **QA & CI/CD:** loop tests
- **Docs & Ops**

---

## Acknowledgements
- Inspired by the needs of sysadmins and Linux installers everywhere.
- Thanks to the open source community for foundational tools and libraries.
