#!/bin/bash

# RAID Provisioning Tool Installer
# Dependencies and Installation Script

# --- Automated Disk/RAID/Bootstrapping Logic ---
auto_fstab_and_esp() {
    log "[auto_fstab_and_esp] Ensuring /target exists and is mounted"
    mkdir -p /target
    mountpoint -q /target || mount /dev/md0 /target || log "[auto_fstab_and_esp] /dev/md0 not mounted at /target (may be OK if not created yet)"

    log "[auto_fstab_and_esp] Generating /target/etc/fstab with UUIDs"
    mkdir -p /target/etc
    > /target/etc/fstab
    for dev in $(lsblk -ln -o NAME,TYPE | awk '$2=="part"{print "/dev/"$1}'); do
        uuid=$(blkid -s UUID -o value "$dev")
        mp=$(lsblk -no MOUNTPOINT "$dev")
        fstype=$(blkid -s TYPE -o value "$dev")
        [ -z "$uuid" ] && continue
        [ -z "$fstype" ] && continue
        # Guess mountpoint if not mounted
        [ -z "$mp" ] && mp="/mnt/$(basename "$dev")"
        echo "UUID=$uuid $mp $fstype defaults 0 2" >> /target/etc/fstab
    done
    log "[auto_fstab_and_esp] fstab written to /target/etc/fstab"

    # UEFI ESP handling
    if efibootmgr &>/dev/null; then
        log "[auto_fstab_and_esp] UEFI detected, creating ESPs on all member disks"
        idx=1
        for disk in $(lsblk -dln -o NAME | grep -E 'sd|nvme|vd'); do
            espdev="/dev/${disk}1"
            if ! blkid "$espdev" | grep -q vfat; then
                log "[auto_fstab_and_esp] Formatting $espdev as ESP"
                mkfs.fat -F32 "$espdev"
            fi
            mountpt="/target/boot/efi"
            [ $idx -gt 1 ] && mountpt="/target/boot/efi$idx"
            mkdir -p "$mountpt"
            mount "$espdev" "$mountpt" || log "[auto_fstab_and_esp] Could not mount $espdev at $mountpt"
            echo "UUID=$(blkid -s UUID -o value "$espdev") $mountpt vfat errors=remount-ro,nofail 0 1" >> /target/etc/fstab
            idx=$((idx+1))
        done
    fi
}

# Ensure required filesystems are mounted into /target for chrooted operations
mount_chroot_binds() {
    log "[mount_chroot_binds] Binding system mounts into /target"
    for m in /dev /dev/pts /proc /sys /run; do
        mkdir -p "/target${m}"
        mountpoint -q "/target${m}" || mount --bind "${m}" "/target${m}" 2>/dev/null || true
    done
    # efivars for UEFI systems
    if [ -d /sys/firmware/efi/efivars ]; then
        mkdir -p /target/sys/firmware/efi/efivars
        mountpoint -q /target/sys/firmware/efi/efivars || mount -t efivarfs efivarfs /target/sys/firmware/efi/efivars 2>/dev/null || true
    fi
}

unmount_chroot_binds() {
    log "[mount_chroot_binds] Unmounting binds from /target"
    # Unmount in reverse order; ignore failures
    umount -l /target/sys/firmware/efi/efivars 2>/dev/null || true
    for m in /run /sys /proc /dev/pts /dev; do
        umount -l "/target${m}" 2>/dev/null || true
    done
}

auto_grub_install() {
    log "[auto_grub_install] Installing GRUB to all boot targets (UEFI and/or BIOS)"
    mount_chroot_binds

    # Safety: ensure /target looks like a root filesystem before attempting chroot ops
    if [ ! -x /target/bin/bash ]; then
        log "[auto_grub_install] Skipping: /target is not a prepared root (missing /bin/bash)"
        unmount_chroot_binds
        return 0
    fi
    if [ ! -x /target/usr/sbin/grub-install ] && [ ! -x /target/sbin/grub-install ] && [ ! -x /target/bin/grub-install ]; then
        log "[auto_grub_install] Skipping: grub-install not found inside /target"
        unmount_chroot_binds
        return 0
    fi

    # UEFI installs (one per ESP mounted at /boot/efi, /boot/efi2, ...)
    if [ -d /target/boot/efi ]; then
        log "[auto_grub_install] Installing GRUB (UEFI)"
        chroot /target grub-install --target=x86_64-efi --efi-directory=/boot/efi --bootloader-id=GRUB || log "[auto_grub_install] UEFI GRUB install failed for /boot/efi"
        idx=2
        while [ -d "/target/boot/efi$idx" ]; do
            chroot /target grub-install --target=x86_64-efi --efi-directory="/boot/efi$idx" --bootloader-id=GRUB$idx || log "[auto_grub_install] UEFI GRUB install failed for /boot/efi$idx"
            idx=$((idx+1))
        done
    fi

    # BIOS installs to each disk (best-effort)
    for disk in $(lsblk -dln -o NAME | grep -E 'sd|nvme|vd'); do
        log "[auto_grub_install] Installing GRUB (BIOS) to /dev/$disk"
        chroot /target grub-install --target=i386-pc --recheck "/dev/$disk" || log "[auto_grub_install] BIOS GRUB install failed for /dev/$disk"
    done

    log "[auto_grub_install] Generating GRUB config in chroot"
    if chroot /target bash -lc 'command -v update-grub >/dev/null 2>&1'; then
        chroot /target update-grub || log "[auto_grub_install] update-grub failed"
    elif chroot /target bash -lc 'command -v grub-mkconfig >/dev/null 2>&1'; then
        chroot /target grub-mkconfig -o /boot/grub/grub.cfg || log "[auto_grub_install] grub-mkconfig failed"
    elif chroot /target bash -lc 'command -v grub2-mkconfig >/dev/null 2>&1'; then
        if chroot /target test -d /boot/grub2; then
            chroot /target grub2-mkconfig -o /boot/grub2/grub.cfg || log "[auto_grub_install] grub2-mkconfig failed (/boot/grub2)"
        else
            chroot /target grub2-mkconfig -o /boot/grub/grub.cfg || log "[auto_grub_install] grub2-mkconfig failed (/boot/grub)"
        fi
    else
        log "[auto_grub_install] No GRUB config generator found (update-grub/grub-mkconfig/grub2-mkconfig)"
    fi

    log "[auto_grub_install] Updating initramfs in chroot"
    if chroot /target bash -lc 'command -v update-initramfs >/dev/null 2>&1'; then
        chroot /target update-initramfs -u || log "[auto_grub_install] update-initramfs failed"
    elif chroot /target bash -lc 'command -v dracut >/dev/null 2>&1'; then
        chroot /target dracut -f || log "[auto_grub_install] dracut failed"
    elif chroot /target bash -lc 'command -v mkinitcpio >/dev/null 2>&1'; then
        chroot /target mkinitcpio -P || log "[auto_grub_install] mkinitcpio failed"
    else
        log "[auto_grub_install] No initramfs tool found (update-initramfs/dracut/mkinitcpio)"
    fi

    # Sync ESPs if multiple present
    if [ -d /target/boot/efi2 ]; then
        log "[auto_grub_install] Syncing ESPs"
        rsync -a --delete /target/boot/efi/ /target/boot/efi2/ || log "[auto_grub_install] ESP sync failed"
    fi

    unmount_chroot_binds
    log "[auto_grub_install] GRUB installation complete"
}


set -e

# Configuration
LIVE_USER="$SUDO_USER"
LIVE_PASSWORD="live"
LOG_FILE="/var/log/raidctl-installer.log"
PROJECT_DIR="$(dirname "$0")/raidctl"
INSTALL_DIR="/usr/local/bin"
CONFIG_DIR="/etc/raidctl"
APP_DIR="/usr/local/share/raidctl"

# Dependencies list
DEPENDENCIES=(
    # Build tools
    "build-essential"
    "pkg-config"

    # RAID and filesystem tools
    "mdadm"
    "lvm2"
    "e2fsprogs"      # ext2, ext3, ext4 support
    "xfsprogs"       # XFS support
    "btrfs-progs"    # Btrfs support
    "reiserfsprogs"  # ReiserFS support
    "jfsutils"       # JFS support
    "ntfs-3g"        # NTFS support
    "dosfstools"     # FAT32 support
    "exfatprogs"     # exFAT support (modern package)
    "grub2"
    "efibootmgr"
    "udev"

    # Partition tools
    "util-linux"
    "gdisk"          # GPT partition editor
    "parted"         # Partition manipulation program
    "gparted"        # GUI partition editor
    "gnome-disk-utility"  # GNOME Disks utility

    # GUI dependencies
    "libgtk-3-dev"
    "libssl-dev"
    "libudev-dev"

    # System tools
    "sudo"
    "curl"
    "wget"
    "policykit-1"  # For pkexec in desktop entry
    "ca-certificates"  # For secure curl connections
)

# Logging function
log() {
    echo "$(date '+%Y-%m-%d %H:%M:%S') - $*" | tee -a "$LOG_FILE"
}

# Error handling
error_exit() {
    log "ERROR: $1"
    exit 1
}

# Check if running as root
if [[ $EUID -ne 0 ]]; then
    error_exit "This script must be run as root"
fi

# Stop running instances of the GUI to avoid "file busy" errors
stop_running_instances() {
    log "Stopping any running RAID control instances..."
    pkill -f raidctl-gui 2>/dev/null || true
    pkill -f raidctl 2>/dev/null || true
    sleep 2
    log "Running instances stopped"
}

# Function to install dependencies
install_dependencies() {
    log "Installing dependencies..."

    # Detect package manager
    if command -v apt-get &> /dev/null; then
        PACKAGE_MANAGER="apt-get"
        UPDATE_CMD="apt-get update"
        INSTALL_CMD="apt-get install -y"
        REMOVE_CMD="apt-get remove -y"
    elif command -v dnf &> /dev/null; then
        PACKAGE_MANAGER="dnf"
        UPDATE_CMD="dnf check-update || true"
        INSTALL_CMD="dnf install -y"
        REMOVE_CMD="dnf remove -y"
    elif command -v pacman &> /dev/null; then
        PACKAGE_MANAGER="pacman"
        UPDATE_CMD="pacman -Sy"
        INSTALL_CMD="pacman -S --noconfirm"
        REMOVE_CMD="pacman -R --noconfirm"
    else
        error_exit "Unsupported package manager"
    fi

    log "Using package manager: $PACKAGE_MANAGER"
    log "Dependencies to install: ${DEPENDENCIES[*]}"

    # Update package list (skip CD-ROM errors in live environment)
    $UPDATE_CMD 2>/dev/null || {
        # If update fails, try disabling CD-ROM sources
        if [ "$PACKAGE_MANAGER" = "apt-get" ]; then
            sed -i '/cdrom:/d' /etc/apt/sources.list 2>/dev/null || true
            $UPDATE_CMD || log "Failed to update package list, continuing anyway"
        else
            $UPDATE_CMD || log "Failed to update package list, continuing anyway"
        fi
    }

    # Install dependencies
    if $INSTALL_CMD "${DEPENDENCIES[@]}"; then
        log "Dependencies installed successfully"
    else
        error_exit "Failed to install dependencies"
    fi
}

# Function to detect and remove existing Rust installations
remove_existing_rust() {
    log "Checking for existing Rust installations..."
    
    # Check if system Rust is installed
    if command -v rustc &> /dev/null && [ -x "/usr/bin/rustc" ]; then
        log "Found system Rust installation at /usr/bin/rustc"
        log "Attempting to remove system Rust installation..."
        
        # Try to remove system Rust packages
        if [ "$PACKAGE_MANAGER" = "apt-get" ]; then
            # List of common Rust packages
            RUST_PACKAGES=("rustc" "cargo" "rust-doc" "rust-src" "rust-std")
            for pkg in "${RUST_PACKAGES[@]}"; do
                if dpkg -l | grep -q "^ii  $pkg "; then
                    log "Removing system package: $pkg"
                    $REMOVE_CMD "$pkg" 2>/dev/null || log "Warning: Could not remove $pkg"
                fi
            done
        elif [ "$PACKAGE_MANAGER" = "dnf" ]; then
            # Try to remove system Rust packages
            $REMOVE_CMD "rust" "cargo" 2>/dev/null || log "Warning: Could not remove system Rust packages"
        elif [ "$PACKAGE_MANAGER" = "pacman" ]; then
            # Try to remove system Rust packages
            $REMOVE_CMD "rust" 2>/dev/null || log "Warning: Could not remove system Rust packages"
        fi
    fi
    
    # Check if rustup is installed
    if command -v rustup &> /dev/null; then
        log "Found existing rustup installation"
        # Check if it's a system installation
        if [ -x "/usr/bin/rustup" ]; then
            log "Removing system rustup installation..."
            if [ "$PACKAGE_MANAGER" = "apt-get" ]; then
                $REMOVE_CMD "rustup" 2>/dev/null || log "Warning: Could not remove system rustup"
            elif [ "$PACKAGE_MANAGER" = "dnf" ]; then
                $REMOVE_CMD "rustup" 2>/dev/null || log "Warning: Could not remove system rustup"
            elif [ "$PACKAGE_MANAGER" = "pacman" ]; then
                $REMOVE_CMD "rustup" 2>/dev/null || log "Warning: Could not remove system rustup"
            fi
        fi
    fi
    
    # Check for user-specific rustup installation
    if [ -d "/home/$LIVE_USER/.rustup" ]; then
        log "Found user rustup installation, removing..."
        rm -rf "/home/$LIVE_USER/.rustup" || log "Warning: Could not remove user rustup directory"
    fi
    
    # Check for user-specific cargo installation
    if [ -d "/home/$LIVE_USER/.cargo" ]; then
        log "Found user cargo installation, removing..."
        rm -rf "/home/$LIVE_USER/.cargo" || log "Warning: Could not remove user cargo directory"
    fi
    
    # Clear PATH of Rust-related entries for this session
    export PATH=$(echo $PATH | tr ':' '\n' | grep -v "\.cargo" | tr '\n' ':' | sed 's/:$//')
    
    log "Existing Rust installations removal process completed"
}

# ... (file continues unchanged below)
