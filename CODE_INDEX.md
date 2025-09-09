# LiveRAID Code Index

## Project Overview

**LiveRAID** is a turnkey, cross-distro RAID provisioning tool for Linux designed for live environments. It provides both modern GUI and CLI interfaces with full automation of RAID, partitioning, fstab, ESP, and GRUB setup.

## Architecture

### Core Components

```
raidctl/
├── raidctl-core/         # Core logic library
├── raidctl-cli/          # CLI frontend  
├── raidctl-gui/          # GUI frontend
└── target/               # Build artifacts
```

## File Structure Index

### Root Level
- **`README.md`** - Project documentation, features, architecture overview
- **`installer.sh`** - Main installation script with automated disk/RAID/bootstrapping logic

### Core Library (`raidctl-core/`)
- **`src/lib.rs`** - Core functionality implementation
  - `RaidError` - Error handling for RAID operations
  - `RaidLevel` - RAID level enumeration (0, 1, 5, 6, 10, None)
  - `Device` - Storage device representation with discovery
  - `Filesystem` - Filesystem type support (ext4/3/2, xfs, btrfs, etc.)
  - `Config` - Configuration management
  - `Planner` - RAID provisioning planner with device discovery
  - `ProvisioningPlan` - Plan execution logic
  - `execute_plan()` - Main execution function for RAID creation

### CLI Frontend (`raidctl-cli/`)
- **`src/main.rs`** - Command-line interface implementation
  - `Cli` - Main CLI structure with clap parser
  - `Commands` - Subcommands: Discover, Plan, Apply
  - `RaidLevelCli` - CLI-specific RAID level enum
  - Device discovery and plan execution workflows

### GUI Frontend (`raidctl-gui/`)
- **`src/main.rs`** - Modern GUI implementation using egui/eframe
  - `RaidCtlApp` - Main application state and UI logic
  - Device selection interface with visual icons
  - RAID level and filesystem selection dropdowns
  - Live environment detection and system tools integration
  - GRUB configuration generation and management
  - Terminal integration and partition tool launching

## Key Data Structures

### RaidLevel Enum
```rust
pub enum RaidLevel {
    None,      // Single disk
    Raid0,     // Striping (min 2 disks)
    Raid1,     // Mirroring (min 2 disks) 
    Raid5,     // Striping with parity (min 3 disks)
    Raid6,     // Double parity (min 4 disks)
    Raid10,    // Mirrored stripes (min 4 disks)
}
```

### Device Structure
```rust
pub struct Device {
    pub id: String,
    pub path: String,
    pub size: u64,
    pub model: Option<String>,
    pub serial: Option<String>,
}
```

### Filesystem Support
- **ext4/ext3/ext2** - Linux journaling filesystems
- **xfs** - High performance filesystem
- **btrfs** - Advanced filesystem with snapshots
- **reiserfs/jfs** - Legacy journaling filesystems
- **ntfs** - Windows compatibility
- **fat32/exfat** - Universal compatibility

## Core Functionality

### Device Discovery
- Uses `lsblk -J` for JSON device information
- Filters mounted devices and non-disk types
- Parses device sizes with unit conversion
- Extracts model and serial information

### RAID Planning
- Validates minimum disk requirements per RAID level
- Checks device availability and paths
- Creates provisioning plans with filesystem selection
- Supports dry-run mode for safe testing

### Execution Pipeline
1. **Device Scan** - Discover available storage devices
2. **Plan Creation** - Validate RAID configuration
3. **RAID Setup** - Execute mdadm commands
4. **Filesystem Creation** - Format with selected filesystem
5. **Mount Configuration** - Create mount points and fstab entries
6. **Boot Setup** - Configure GRUB and initramfs

## Installation System

### installer.sh Features
- **Dependency Management** - Installs build tools, RAID tools, filesystem utilities
- **Cross-distro Support** - Detects apt-get, dnf, or pacman
- **Rust Installation** - Manages Rust toolchain with version checking
- **Build Process** - Compiles release binaries with proper permissions
- **System Integration** - Creates desktop entries and configuration files
- **Automated Bootstrapping** - Handles fstab, ESP, and GRUB setup

### Dependencies
```bash
# Build Tools
build-essential, pkg-config, rust

# RAID & Filesystem Tools  
mdadm, lvm2, e2fsprogs, xfsprogs, btrfs-progs
reiserfsprogs, jfsutils, ntfs-3g, dosfstools, exfatprogs

# Boot & Partition Tools
grub2, efibootmgr, util-linux, gdisk, parted

# GUI Dependencies
libgtk-3-dev, libssl-dev, libudev-dev
```

## Configuration

### Default Config (`/etc/raidctl/config.toml`)
```toml
[general]
log_level = "info"

[storage]
dry_run = true
backup_existing_configs = true
target_mount = "/target"
grub_timeout = 5
```

## Safety Features

- **Dry-run Mode** - Preview operations without execution
- **Device Validation** - Prevents using mounted or system devices
- **Minimum Disk Checks** - Validates RAID level requirements
- **Configuration Backup** - Preserves existing system configs
- **Error Handling** - Comprehensive error reporting and rollback
- **Live Environment Detection** - Adapts behavior for live vs installed systems

## Build System

### Cargo Workspace Configuration
- **Edition**: Rust 2021
- **Shared Dependencies**: serde, toml, clap, anyhow, thiserror, log
- **GUI Dependencies**: eframe, egui, chrono
- **Build Optimization**: Release builds with proper permissions

### Build Commands
```bash
# Development build
cargo build

# Release build (used by installer)
cargo build --release

# Clean build artifacts
cargo clean
```

## Integration Points

### System Tools Integration
- **fdisk/parted** - Partition management
- **GParted** - GUI partition editor
- **GNOME Disks** - System disk utility
- **Terminal Emulation** - Command execution with output capture

### Boot System Integration
- **UEFI Support** - ESP creation and management
- **BIOS Support** - MBR and bios_grub partitions
- **GRUB Configuration** - Automated bootloader setup
- **Initramfs Integration** - Cross-distro initramfs handling

## Error Handling

### RaidError Types
- `DeviceNotFound` - Invalid device paths
- `InvalidRaidLevel` - Unsupported RAID configuration
- `InsufficientDisks` - Not enough disks for RAID level
- `IoError` - System I/O failures

## Usage Patterns

### CLI Workflow
```bash
# Discover devices
raidctl discover

# Plan RAID configuration
raidctl plan --level raid1 /dev/sda /dev/sdb

# Execute plan
raidctl apply plan.json
```

### GUI Workflow
1. Launch `raidctl-gui`
2. Select storage devices from visual interface
3. Choose RAID level and filesystem
4. Preview configuration
5. Apply changes with confirmation

This index provides a comprehensive overview of the LiveRAID codebase structure, functionality, and integration points for developers and system administrators.
