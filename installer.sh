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
        [ -z "$mp" ] && mp="/mnt/$dev"
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

auto_grub_install() {
    log "[auto_grub_install] Installing GRUB to all boot targets (UEFI and/or BIOS)"
    # Chroot to /target for all bootloader commands
    if [ -d /target/boot/efi ]; then
        log "[auto_grub_install] Installing GRUB (UEFI)"
        chroot /target grub-install --target=x86_64-efi --efi-directory=/boot/efi --bootloader-id=GRUB || log "[auto_grub_install] UEFI GRUB install failed"
        idx=2
        while [ -d "/target/boot/efi$idx" ]; do
            chroot /target grub-install --target=x86_64-efi --efi-directory="/boot/efi$idx" --bootloader-id=GRUB$idx || log "[auto_grub_install] UEFI GRUB install failed for efi$idx"
            idx=$((idx+1))
        done
    fi
    for disk in $(lsblk -dln -o NAME | grep -E 'sd|nvme|vd'); do
        log "[auto_grub_install] Installing GRUB (BIOS) to /dev/$disk"
        chroot /target grub-install --target=i386-pc --recheck "/dev/$disk" || log "[auto_grub_install] BIOS GRUB install failed for /dev/$disk"
    done
    log "[auto_grub_install] Running update-grub and update-initramfs in chroot"
    chroot /target update-grub || log "[auto_grub_install] update-grub failed"
    chroot /target update-initramfs -u || log "[auto_grub_install] update-initramfs failed"
    # Sync ESPs if UEFI
    if [ -d /target/boot/efi2 ]; then
        log "[auto_grub_install] Syncing ESPs"
        rsync -a --delete /target/boot/efi/ /target/boot/efi2/ || log "[auto_grub_install] ESP sync failed"
    fi
    log "[auto_grub_install] GRUB installation complete"
}


set -e

# Configuration
LIVE_USER="$SUDO_USER"
LIVE_PASSWORD="live"
LOG_FILE="/var/log/raidctl-installer.log"
PROJECT_DIR="/home/$SUDO_USER/Desktop/mdadm_RAIDlive/raidctl"
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

# Function to compare versions
version_greater_equal() {
    local v1=$1
    local v2=$2
    
    # Convert versions to comparable format
    local v1_nums=$(echo "$v1" | tr '.' ' ')
    local v2_nums=$(echo "$v2" | tr '.' ' ')
    
    # Compare each part of the version
    set -- $v1_nums
    local v1_major=${1:-0}
    local v1_minor=${2:-0}
    local v1_patch=${3:-0}
    
    set -- $v2_nums
    local v2_major=${1:-0}
    local v2_minor=${2:-0}
    local v2_patch=${3:-0}
    
    # Compare major version
    if [ "$v1_major" -gt "$v2_major" ]; then
        return 0
    elif [ "$v1_major" -lt "$v2_major" ]; then
        return 1
    fi
    
    # Compare minor version
    if [ "$v1_minor" -gt "$v2_minor" ]; then
        return 0
    elif [ "$v1_minor" -lt "$v2_minor" ]; then
        return 1
    fi
    
    # Compare patch version
    if [ "$v1_patch" -ge "$v2_patch" ]; then
        return 0
    else
        return 1
    fi
}

# Function to install Rust if not present or if version is too old
install_rust() {
    local MIN_RUST_VERSION="1.81.0"
    
    # Check if Rust is already installed with sufficient version
    if command -v rustc &> /dev/null; then
        local CURRENT_VERSION=$(rustc --version | awk '{print $2}')
        log "Found existing Rust installation. Version: $CURRENT_VERSION"
        
        # Check if version is sufficient
        if version_greater_equal "$CURRENT_VERSION" "$MIN_RUST_VERSION"; then
            log "Existing Rust version is sufficient, keeping it"
            # Source the environment if it exists
            if [ -f "/home/$LIVE_USER/.cargo/env" ]; then
                source "/home/$LIVE_USER/.cargo/env" 2>/dev/null || true
            fi
            export PATH="/home/$LIVE_USER/.cargo/bin:$PATH"
            return 0
        else
            log "Existing Rust version is too old, will install newer version"
        fi
    fi
    
    log "Installing latest Rust using rustup..."
    
    # Install Rust using rustup
    if ! sudo -u "$LIVE_USER" bash -c "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y"; then
        error_exit "Failed to install Rust"
    fi
    
    # Set the default toolchain for liveuser
    log "Setting default Rust toolchain for $LIVE_USER..."
    if ! sudo -u "$LIVE_USER" bash -c "source \"\$HOME/.cargo/env\" 2>/dev/null && rustup default stable"; then
        log "Warning: Failed to set default Rust toolchain for $LIVE_USER"
    fi
    
    # Also set for root user if running as root
    if [ "$USER" = "root" ] && [ -f "/root/.cargo/env" ]; then
        log "Setting default Rust toolchain for root..."
        if ! bash -c "source \"/root/.cargo/env\" 2>/dev/null && rustup default stable"; then
            log "Warning: Failed to set default Rust toolchain for root"
        fi
    fi
    
    # Source the environment
    if [ -f "/home/$LIVE_USER/.cargo/env" ]; then
        source "/home/$LIVE_USER/.cargo/env" 2>/dev/null || true
        export PATH="/home/$LIVE_USER/.cargo/bin:$PATH"
    fi
    
    # Add to profile for future sessions
    echo 'source "$HOME/.cargo/env"' >> "/home/$LIVE_USER/.profile"
    
    # Verify installation
    if command -v rustc &> /dev/null; then
        local RUST_VERSION=$(rustc --version | awk '{print $2}')
        log "Rust installed successfully. Version: $RUST_VERSION"
        export PATH="/home/$LIVE_USER/.cargo/bin:$PATH"
    else
        error_exit "Rust installation failed"
    fi
    
    log "Rust and Cargo installed successfully"
}


# Function to build the project
build_project() {
    log "Building RAID Provisioning Tool..."
    
    # Debug: Print current user and environment
    log "Current user: $(whoami)"
    log "LIVE_USER: $LIVE_USER"
    log "PROJECT_DIR: $PROJECT_DIR"
    log "PATH: $PATH"
    
    # Ensure we're using the correct Rust version
    export PATH="/home/$LIVE_USER/.cargo/bin:$PATH"
    if [ -f "/home/$LIVE_USER/.cargo/env" ]; then
        log "Sourcing Rust environment..."
        source "/home/$LIVE_USER/.cargo/env" 2>/dev/null || true
        log "Rust environment sourced"
    fi
    
    # Debug: Print PATH after sourcing Rust environment
    log "PATH after sourcing Rust environment: $PATH"
    
    # Set the default toolchain and verify Rust installation
    log "Setting default Rust toolchain to stable..."
    if ! sudo -u "$LIVE_USER" bash -c 'source "$HOME/.cargo/env" 2>/dev/null && rustup default stable'; then
        error_exit "Failed to set default Rust toolchain"
    fi
    
    # Verify Rust version
    if sudo -u "$LIVE_USER" bash -c 'source "$HOME/.cargo/env" 2>/dev/null && command -v rustc &> /dev/null'; then
        local RUST_VERSION=$(sudo -u "$LIVE_USER" bash -c 'source "$HOME/.cargo/env" 2>/dev/null && rustc --version' | awk '{print $2}')
        log "Using Rust version: $RUST_VERSION"
        
        # Verify cargo is available
        if ! sudo -u "$LIVE_USER" bash -c 'source "$HOME/.cargo/env" 2>/dev/null && command -v cargo &> /dev/null'; then
            error_exit "Cargo not found for liveuser"
        fi
    else
        error_exit "Rust not found for liveuser"
    fi
    
    # Check if project directory exists and has correct ownership
    if [ ! -d "$PROJECT_DIR" ]; then
        error_exit "Project directory not found: $PROJECT_DIR"
    fi
    
    # Ensure proper ownership and permissions on project directory
    log "Setting correct permissions on project directory..."
    
    # Set directory permissions (755 for directories, 644 for files)
    find "$PROJECT_DIR" -type d -exec chmod 755 {} \; || log "Warning: Could not set directory permissions"
    find "$PROJECT_DIR" -type f -exec chmod 644 {} \; || log "Warning: Could not set file permissions"
    
    # Make scripts executable
    find "$PROJECT_DIR" -name "*.sh" -exec chmod +x {} \; || log "Warning: Could not set executable permissions on scripts"
    
    # Set ownership to current user during installation
    chown -R "$USER":"$USER" "$PROJECT_DIR" || log "Warning: Could not set ownership of project directory"
    
    # Special handling for Cargo.lock
    if [ -f "$PROJECT_DIR/Cargo.lock" ]; then
        log "Updating Cargo.lock permissions..."
        chmod 644 "$PROJECT_DIR/Cargo.lock"
    fi
    
    # Ensure target directory exists and has correct permissions
    mkdir -p "$PROJECT_DIR/target"
    chmod 755 "$PROJECT_DIR/target"
    
    # Clean previous build artifacts with proper permissions
    log "Cleaning previous build artifacts..."
    (cd "$PROJECT_DIR" && cargo clean 2>/dev/null) || true
    
    # Ensure target directory has correct permissions
    if [ -d "$PROJECT_DIR/target" ]; then
        chmod -R 755 "$PROJECT_DIR/target"
        find "$PROJECT_DIR/target" -type f -exec chmod 644 {} \;
    fi
    
    # Build the project with proper permissions and Rust environment
    log "Compiling project in release mode..."
    
    # Ensure liveuser can access the project directory
    chown -R "$LIVE_USER":"$LIVE_USER" "$PROJECT_DIR"
    
    # Build with proper Rust environment (use current user's Rust installation)
    if (cd "$PROJECT_DIR" && source "/home/$LIVE_USER/.cargo/env" 2>/dev/null && rustup default stable && cargo build --release); then
        log "Build completed successfully"
        
        # Set correct permissions on build artifacts
        if [ -d "$PROJECT_DIR/target/release" ]; then
            find "$PROJECT_DIR/target/release" -type f -exec chmod 755 {} \;
            find "$PROJECT_DIR/target/release" -type d -exec chmod 755 {} \;
        fi
        
        # Verify that the binaries were built
        if [ -f "$PROJECT_DIR/target/release/raidctl" ] && [ -f "$PROJECT_DIR/target/release/raidctl-gui" ]; then
            log "RAID control binaries found"
            # Ensure binaries are executable
            chmod +x "$PROJECT_DIR/target/release/raidctl"
            chmod +x "$PROJECT_DIR/target/release/raidctl-gui"
        else
            error_exit "RAID control binaries not found after build"
        fi
    else
        error_exit "Failed to build project"
    fi
}

# Function to install binaries
install_binaries() {
    log "Installing binaries..."

    # Stop any running instances to avoid "file busy" errors
    stop_running_instances

    # Create installation directories with proper permissions
    mkdir -p "$INSTALL_DIR"
    chmod 755 "$INSTALL_DIR"
    
    # Create config directory with proper permissions
    mkdir -p "$CONFIG_DIR"
    chmod 755 "$CONFIG_DIR"
    
    # Create app directory with proper permissions
    mkdir -p "$APP_DIR"
    chmod 755 "$APP_DIR"

    # Check if binaries were built successfully
    if [ ! -f "$PROJECT_DIR/target/release/raidctl" ]; then
        error_exit "CLI binary not found. Build may have failed."
    fi
    
    if [ ! -f "$PROJECT_DIR/target/release/raidctl-gui" ]; then
        error_exit "GUI binary not found. Build may have failed."
    fi

    log "Installing RAID control binaries..."
    
    # Copy and set permissions for CLI binary
    if [ -f "$PROJECT_DIR/target/release/raidctl" ]; then
        cp "$PROJECT_DIR/target/release/raidctl" "$INSTALL_DIR/"
        chmod 755 "$INSTALL_DIR/raidctl"
        chown root:root "$INSTALL_DIR/raidctl"
        log "Installed raidctl to $INSTALL_DIR/raidctl"
    else
        error_exit "Failed to find raidctl binary for installation"
    fi
    
    # Copy and set permissions for GUI binary
    if [ -f "$PROJECT_DIR/target/release/raidctl-gui" ]; then
        cp "$PROJECT_DIR/target/release/raidctl-gui" "$INSTALL_DIR/"
        chmod 755 "$INSTALL_DIR/raidctl-gui"
        chown root:root "$INSTALL_DIR/raidctl-gui"
        log "Installed raidctl-gui to $INSTALL_DIR/raidctl-gui"
    else
        error_exit "Failed to find raidctl-gui binary for installation"
    fi

    log "Binaries installed successfully"
}

# Function to create desktop entry
create_desktop_entry() {
    log "Creating desktop entry..."

    # Create applications directory if it doesn't exist with proper permissions
    mkdir -p /usr/share/applications
    chmod 755 /usr/share/applications

    local DESKTOP_FILE="/usr/share/applications/raidctl.desktop"
    cat > "$DESKTOP_FILE" <<EOL
[Desktop Entry]
Type=Application
Name=RAID Control
Comment=RAID Configuration Tool
Exec=$INSTALL_DIR/raidctl-gui
Icon=$APP_DIR/icon.png
Terminal=false
Categories=System;Utility;
EOL

    # Set proper permissions for desktop file
    chmod 644 "$DESKTOP_FILE"
    chown root:root "$DESKTOP_FILE"
    
    log "Desktop entry created at $DESKTOP_FILE"
}

# Function to create configuration
create_config() {
    log "Creating configuration..."

    # Create config directory with proper permissions
    mkdir -p "$CONFIG_DIR"
    chmod 755 "$CONFIG_DIR"
    chown root:root "$CONFIG_DIR"

    # Only create config if it doesn't already exist
    if [ ! -f "$CONFIG_DIR/config.toml" ]; then
        # Create default config with proper permissions
        local CONFIG_FILE="$CONFIG_DIR/config.toml"
        cat > "$CONFIG_FILE" <<EOL
[general]
log_level = "info"

[storage]
# Default storage configuration
dry_run = true
backup_existing_configs = true
target_mount = "/target"
grub_timeout = 5
EOL
        
        # Set proper permissions for config file
        chmod 644 "$CONFIG_FILE"
        chown root:root "$CONFIG_FILE"
        log "Default configuration created at $CONFIG_FILE"
    else
        log "Configuration already exists, skipping creation"
    fi
}

# Function to verify installation
verify_installation() {
    log "Verifying installation..."
    local success=true

    # Check if binaries are installed, executable, and have correct permissions
    if command -v raidctl &> /dev/null; then
        log "CLI tool installed successfully: $(which raidctl)"
        # Test CLI help command
        if raidctl --help &> /dev/null; then
            log "CLI tool is functional"
        else
            log "Warning: CLI tool may not be functional"
        fi
    else
        log "Warning: CLI tool not found in PATH"
    fi
    
    if command -v raidctl-gui &> /dev/null; then
        log "GUI tool installed successfully: $(which raidctl-gui)"
    else
        log "Warning: GUI tool not found in PATH"
    fi
    
    # Check if configuration exists
    if [ -f "$CONFIG_DIR/config.toml" ]; then
        log "Configuration file exists"
    else
        log "Warning: Configuration file not found"
    fi
    
    # Check if desktop entry exists
    if [ -f "/usr/share/applications/raidctl.desktop" ]; then
        log "Desktop entry exists"
    else
        log "Warning: Desktop entry not found"
    fi
    
    log "Installation verification completed"
}

# Function to start GUI
start_gui() {
    log "Starting RAID Provisioning Tool GUI..."

    # Check if display is available
    if [[ -z "$DISPLAY" ]]; then
        log "No display available, GUI cannot be started"
        log "You can run the CLI version with: raidctl --help"
        return
    fi

    # Start GUI
    /usr/local/bin/raidctl-gui &

    log "GUI started"
}

# Function to cleanup temporary files
cleanup() {
    log "Cleaning up temporary files..."
    
    # Clean cargo cache
    if command -v cargo &> /dev/null; then
        su - "$LIVE_USER" -c "cargo cache --autoclean" 2>/dev/null || true
    fi
    
    # Remove build artifacts to save space
    if [ -d "$PROJECT_DIR/target" ]; then
        rm -rf "$PROJECT_DIR/target"
        log "Build artifacts removed"
    fi
    
    log "Cleanup completed"
}

# Main installation function
main() {
    log "Starting RAID Provisioning Tool installation"

    # Create log file
    touch "$LOG_FILE"
    chmod 644 "$LOG_FILE"

    # Check if liveuser exists
    if ! id "$LIVE_USER" &> /dev/null; then
        useradd -m -s /bin/bash "$LIVE_USER" || error_exit "Failed to create liveuser"
        echo "$LIVE_USER:$LIVE_PASSWORD" | chpasswd || error_exit "Failed to set liveuser password"
        log "Created liveuser: $LIVE_USER"
    fi

    # Set up project directory with proper permissions and ownership
    if [ ! -d "$PROJECT_DIR" ]; then
        error_exit "Project directory not found: $PROJECT_DIR"
    fi
    
    log "Setting up project directory permissions..."
    
    # Set directory permissions (755 for directories, 644 for files)
    find "$PROJECT_DIR" -type d -exec chmod 755 {} \; || log "Warning: Could not set directory permissions"
    find "$PROJECT_DIR" -type f -exec chmod 644 {} \; || log "Warning: Could not set file permissions"
    
    # Make scripts executable
    find "$PROJECT_DIR" -name "*.sh" -exec chmod +x {} \; || log "Warning: Could not set executable permissions on scripts"
    
    # Set ownership to current user during installation
    chown -R "$USER":"$USER" "$PROJECT_DIR" || log "Warning: Could not set ownership of project directory"

    # Install dependencies
    install_dependencies

    # Remove any existing Rust installations
    remove_existing_rust

    # Install Rust
    install_rust

    # Build project
    build_project

    # Install binaries
    install_binaries

    # Create desktop entry
    create_desktop_entry

    # Create configuration
    create_config

    # Verify installation
    verify_installation

    # Cleanup temporary files
    cleanup

    # Final Rust toolchain setup
    log "Performing final Rust toolchain setup..."
    
    # Ensure rustup default stable is set for all users
    if sudo -u "$LIVE_USER" bash -c 'source "$HOME/.cargo/env" 2>/dev/null && rustup default stable'; then
        log "Rust toolchain set to stable for $LIVE_USER"
    else
        log "Warning: Could not set Rust toolchain for $LIVE_USER"
    fi
    
    # Set for current user if different from liveuser
    if [ "$USER" != "$LIVE_USER" ] && [ -f "$HOME/.cargo/env" ]; then
        if bash -c 'source "$HOME/.cargo/env" 2>/dev/null && rustup default stable'; then
            log "Rust toolchain set to stable for $USER"
        else
            log "Warning: Could not set Rust toolchain for $USER"
        fi
    fi

    log "Installation completed successfully"

    # Start GUI
    start_gui

    # Automated disk/RAID/bootstrapping steps
    log "[main] Running auto_fstab_and_esp (disk, fstab, ESP setup)"
    auto_fstab_and_esp
    log "[main] Running auto_grub_install (bootloader setup)"
    auto_grub_install

    log "RAID Provisioning Tool is ready to use!"
    log "CLI: raidctl --help"
    log "GUI: raidctl-gui"
    log ""
    log "Note: If you encounter Rust toolchain issues, run:"
    log "  source \"\$HOME/.cargo/env\" && rustup default stable"
}

# Function to finalize Rust setup
finalize_rust_setup() {
    log "Finalizing Rust toolchain setup..."
    
    # Ensure rustup is in PATH
    export PATH="/home/$LIVE_USER/.cargo/bin:$PATH"
    
    # Set default toolchain for the current user
    if ! sudo -u "$LIVE_USER" bash -c 'source "$HOME/.cargo/env" 2>/dev/null && rustup default stable'; then
        log "Warning: Failed to set default Rust toolchain for liveuser"
    fi
    
    # Also set for root user if different from liveuser
    if [ "$USER" != "$LIVE_USER" ]; then
        if ! bash -c 'source "$HOME/.cargo/env" 2>/dev/null && rustup default stable'; then
            log "Warning: Failed to set default Rust toolchain for $USER"
        fi
    fi
    
    log "Rust toolchain setup complete"
}

# Run main function
main "$@"

# Final Rust setup
echo
log "Installation completed. Please run the following command to ensure Rust is properly set up:"
echo "source \"$HOME/.cargo/env\" && rustup default stable"
log "Or log out and log back in for the changes to take effect."
