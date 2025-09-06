//! Graphical interface for the RAID Provisioning Tool

use anyhow::Result;
use eframe::egui;
use raidctl_core::{Device, Planner, RaidLevel};
use std::collections::HashSet;
use std::fs::File;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Format bytes into human-readable size (GB, TB, etc.)
fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB", "PB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;
    
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    
    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.2} {}", size, UNITS[unit_index])
    }
}

/// Available filesystem types for RAID arrays
const FILESYSTEM_TYPES: &[&str] = &[
    "ext4",
    "xfs", 
    "btrfs",
    "ext3",
    "ext2",
    "reiserfs",
    "jfs",
    "ntfs",
    "fat32",
    "exfat"
];

/// Backup GRUB configuration before making changes
fn backup_grub_config() -> Result<String> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let backup_path = format!("/etc/default/grub.backup.{}", timestamp);
    
    std::fs::copy("/etc/default/grub", &backup_path)?;
    Ok(backup_path)
}

/// Main application structure
struct RaidCtlApp {
    devices: Arc<Mutex<Vec<Device>>>,
    selected_devices: HashSet<String>,
    selected_raid_level: Option<RaidLevel>,
    selected_filesystem: Option<String>,
    bootable_flag: bool,
    status: String,
    refresh_requested: bool,
    show_grub_config: bool,
    grub_config: String,
}

impl Default for RaidCtlApp {
    fn default() -> Self {
        let mut app = Self {
            devices: Arc::new(Mutex::new(Vec::new())),
            selected_devices: HashSet::new(),
            selected_raid_level: None,
            selected_filesystem: Some("ext4".to_string()),
            bootable_flag: false,
            status: "Ready".to_string(),
            refresh_requested: true,
            show_grub_config: false,
            grub_config: String::new(),
        };
        
        // Load existing GRUB configuration on startup
        app.load_existing_grub_config();
        app
    }
}

impl eframe::App for RaidCtlApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Configure larger font sizes for better readability
        
        // Set larger text sizes
        let mut style = (*ctx.style()).clone();
        style.text_styles = [
            (egui::TextStyle::Heading, egui::FontId::new(24.0, egui::FontFamily::Proportional)),
            (egui::TextStyle::Body, egui::FontId::new(16.0, egui::FontFamily::Proportional)),
            (egui::TextStyle::Monospace, egui::FontId::new(14.0, egui::FontFamily::Monospace)),
            (egui::TextStyle::Button, egui::FontId::new(16.0, egui::FontFamily::Proportional)),
            (egui::TextStyle::Small, egui::FontId::new(12.0, egui::FontFamily::Proportional)),
        ].into();
        ctx.set_style(style);
        
        // Request a repaint to keep the UI responsive
        ctx.request_repaint_after(Duration::from_millis(100));
        
        // Refresh devices if requested
        if self.refresh_requested {
            self.refresh_devices(ctx);
            self.refresh_requested = false;
        }
        
        // Get and sort devices by size (descending)
        let devices = {
            let mut devices = self.devices.lock().unwrap().clone();
            devices.sort_by(|a, b| b.size.cmp(&a.size));
            devices
        };
        
        let selected_devices: Vec<String> = self.selected_devices.iter().cloned().collect();

        egui::CentralPanel::default().show(ctx, |ui| {
            // Main header with title and refresh button
            ui.vertical_centered(|ui| {
                ui.heading("RAID Provisioning Tool");
                ui.add_space(10.0);
                
                // Status bar with refresh button
                ui.horizontal(|ui| {
                    if ui.button("🔄 Refresh Devices").clicked() {
                        self.refresh_requested = true;
                    }
                    ui.separator();
                    ui.label(&self.status);
                });
                
                ui.separator();
            });
            
            // Main device grid with RAID level and filesystem selection at the top
            ui.vertical(|ui| {
                // RAID Level and Filesystem Type in one row
                ui.horizontal(|ui| {
                    // RAID Level column
                    ui.vertical(|ui| {
                        ui.heading("RAID Level");
                        egui::Grid::new("raid_grid")
                            .num_columns(3)
                            .spacing([10.0, 5.0])
                            .show(ui, |ui| {
                                for (i, level) in RaidLevel::all().iter().enumerate() {
                                    let is_selected = self.selected_raid_level.as_ref() == Some(level);
                                    let response = ui.radio(is_selected, level.display_name());
                                    
                                    if response.clicked() {
                                        self.selected_raid_level = Some(level.clone());
                                    }
                                    
                                    // New row every 3 items
                                    if (i + 1) % 3 == 0 {
                                        ui.end_row();
                                    }
                                }
                            });
                    });
                    
                    ui.separator();
                    
                    // Filesystem Type column
                    ui.vertical(|ui| {
                        ui.heading("Filesystem Type");
                        egui::Grid::new("filesystem_grid")
                            .num_columns(5)
                            .spacing([10.0, 5.0])
                            .show(ui, |ui| {
                                for (i, fs_type) in FILESYSTEM_TYPES.iter().enumerate() {
                                    let is_selected = self.selected_filesystem.as_ref() == Some(&fs_type.to_string());
                                    let response = ui.radio(is_selected, *fs_type);
                                    
                                    if response.clicked() {
                                        self.selected_filesystem = Some(fs_type.to_string());
                                    }
                                    
                                    // New row every 5 items
                                    if (i + 1) % 5 == 0 {
                                        ui.end_row();
                                    }
                                }
                            });
                    });
                });
                
                ui.add_space(10.0);
                
                // Boot flag option
                ui.horizontal(|ui| {
                    let response = ui.checkbox(&mut self.bootable_flag, "Make RAID array bootable");
                    if response.changed() {
                        if self.bootable_flag {
                            self.status = "RAID will be configured as bootable".to_string();
                        } else {
                            self.status = "RAID will be configured as non-bootable".to_string();
                        }
                    }
                    ui.label("ℹ Enable this to configure GRUB for booting from the RAID array");
                });

                // Show requirements for the selected RAID level
                if let Some(raid_level) = &self.selected_raid_level {
                    let min_disks = raid_level.min_disks();
                    let selected_count = self.selected_devices.len();
                    
                    // Show validation message
                    if selected_count < min_disks {
                        ui.colored_label(egui::Color32::YELLOW, 
                            format!("⚠ {} requires at least {} disks (selected: {})", 
                                  raid_level.display_name(), min_disks, selected_count)
                        );
                    } else {
                        ui.colored_label(egui::Color32::GREEN,
                            format!("✓ {}: {} disks selected (minimum: {})", 
                                  raid_level.display_name(), selected_count, min_disks)
                        );
                    }
                    
                    ui.add_space(10.0);
                }
                
                // Device selection
                ui.heading("Available Storage Devices");
                
                if devices.is_empty() {
                    ui.label("No storage devices found. Click 'Refresh Devices' to scan.");
                } else {
                    // Get the selection color before creating the grid
                    let _selection_color = ui.style().visuals.selection.bg_fill;
                    
                    egui::Grid::new("devices_grid")
                        .num_columns(2)
                        .spacing([20.0, 10.0])
                        .show(ui, |ui| {
                            for device in devices.iter() {
                                let is_selected = self.selected_devices.contains(&device.path);
                                
                                // Device icon and selection button
                                ui.horizontal(|ui| {
                                    // Device icon that changes color when selected
                                    let icon_color = if is_selected { 
                                        egui::Color32::from_rgb(0, 150, 255) // Blue when selected
                                    } else { 
                                        egui::Color32::GRAY 
                                    };
                                    
                                    let icon_text = if device.path.contains("nvme") {
                                        "💾" // NVMe SSD icon
                                    } else if device.path.contains("sd") {
                                        "🗄️" // SATA/SCSI drive icon
                                    } else {
                                        "💿" // Generic disk icon
                                    };
                                    
                                    let icon_button = egui::Button::new(
                                        egui::RichText::new(icon_text)
                                            .size(24.0)
                                            .color(icon_color)
                                    )
                                    .fill(egui::Color32::TRANSPARENT)
                                    .frame(false);
                                    
                                    if ui.add(icon_button).clicked() {
                                        if is_selected {
                                            self.selected_devices.remove(&device.path);
                                        } else {
                                            self.selected_devices.insert(device.path.clone());
                                        }
                                    }
                                    
                                    // Device path button
                                    let path_color = if is_selected {
                                        egui::Color32::from_rgb(0, 200, 100) // Green when selected
                                    } else {
                                        egui::Color32::WHITE
                                    };
                                    
                                    let button = egui::Button::new(
                                        egui::RichText::new(&device.path)
                                            .color(path_color)
                                    )
                                    .fill(if is_selected { 
                                        egui::Color32::from_rgba_premultiplied(0, 100, 50, 100) 
                                    } else { 
                                        egui::Color32::from_rgba_premultiplied(50, 50, 50, 50) 
                                    })
                                    .min_size(egui::vec2(150.0, 0.0));
                                    
                                    if ui.add(button).clicked() {
                                        if is_selected {
                                            self.selected_devices.remove(&device.path);
                                        } else {
                                            self.selected_devices.insert(device.path.clone());
                                        }
                                    }
                                });
                                
                                // Show device details
                                ui.vertical(|ui| {
                                    if let Some(model) = &device.model {
                                        ui.label(format!("Model: {}", model));
                                    }
                                    if let Some(serial) = &device.serial {
                                        ui.label(format!("Serial: {}", serial));
                                    }
                                    ui.label(format!("Size: {}", format_size(device.size)));
                                });
                                
                                ui.end_row();
                            }
                        });
                }
                
                // Action buttons at the bottom
                ui.separator();
                ui.add_space(10.0);
                
                // Store status messages and state changes
                let mut status_msg = None;
                let mut grub_config = None;
                let mut show_grub = None;
                
                ui.horizontal(|ui| {
                    if ui.button("Plan").clicked() {
                        if let Some(raid_level) = &self.selected_raid_level {
                            if !selected_devices.is_empty() {
                                // Verify configuration before planning
                                match self.verify_boot_configuration(&raid_level, &selected_devices) {
                                    Ok(_) => {
                                        let raid_level_clone = raid_level.clone();
                                        let filesystem = self.selected_filesystem.as_ref().map(|s| s.as_str()).unwrap_or("ext4");
                                        status_msg = Some(format!("✅ Plan created for {} with {} devices using {} filesystem", 
                                            raid_level.display_name(), 
                                            selected_devices.len(),
                                            filesystem));
                                        
                                        // Generate a preview of the GRUB config
                                        match self.generate_grub_config(&raid_level_clone, &selected_devices) {
                                            Ok(config) => {
                                                grub_config = Some(config);
                                                show_grub = Some(true);
                                            }
                                            Err(e) => {
                                                status_msg = Some(format!("❌ Error generating GRUB config: {}", e));
                                                eprintln!("GRUB config generation error: {}", e);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        status_msg = Some(format!("❌ Configuration error: {}", e));
                                        eprintln!("Configuration verification error: {}", e);
                                    }
                                }
                            } else {
                                status_msg = Some("⚠️ Please select at least one device".to_string());
                            }
                        } else {
                            status_msg = Some("⚠️ Please select a RAID level".to_string());
                        }
                    }
                    
                    // Apply button
                    if ui.button("Apply").clicked() {
                        if let Some(raid_level) = &self.selected_raid_level {
                            if !selected_devices.is_empty() {
                                // Verify configuration before applying
                                match self.verify_boot_configuration(&raid_level, &selected_devices) {
                                    Ok(_) => {
                                        let raid_level_clone = raid_level.clone();
                                        let filesystem = self.selected_filesystem.clone().unwrap_or_else(|| "ext4".to_string());
                                        status_msg = Some("🔄 Applying RAID configuration...".to_string());
                                        
                                        match self.apply_raid_config(&raid_level_clone, &selected_devices) {
                                            Ok(_) => {
                                                status_msg = Some("✅ RAID configuration applied successfully".to_string());
                                                // Refresh devices after successful application
                                                self.refresh_requested = true;
                                            }
                                            Err(e) => {
                                                status_msg = Some(format!("❌ Error applying RAID config: {}", e));
                                                eprintln!("RAID application error: {}", e);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        status_msg = Some(format!("❌ Configuration error: {}", e));
                                        eprintln!("Configuration verification error: {}", e);
                                    }
                                }
                            } else {
                                status_msg = Some("⚠️ Please select at least one device".to_string());
                            }
                        } else {
                            status_msg = Some("⚠️ Please select a RAID level".to_string());
                        }
                    }
                    
                    // Dry Run button
                    if ui.button("Dry Run").clicked() {
                        if let Some(raid_level) = &self.selected_raid_level {
                            if !selected_devices.is_empty() {
                                let raid_level_clone = raid_level.clone();
                                let filesystem = self.selected_filesystem.as_ref().map(|s| s.as_str()).unwrap_or("ext4");
                                status_msg = Some(format!("✅ Dry run completed for {} with {} filesystem (no changes made)", 
                                    raid_level_clone.display_name(), filesystem));
                                
                                // Generate a preview of the GRUB config
                                match self.generate_grub_config(&raid_level_clone, &selected_devices) {
                                    Ok(config) => {
                                        grub_config = Some(config);
                                        show_grub = Some(true);
                                    }
                                    Err(e) => {
                                        status_msg = Some(format!("❌ Error generating GRUB config: {}", e));
                                        eprintln!("GRUB config generation error: {}", e);
                                    }
                                }
                            } else {
                                status_msg = Some("⚠️ Please select at least one device".to_string());
                            }
                        } else {
                            status_msg = Some("⚠️ Please select a RAID level".to_string());
                        }
                    }
                    
                    // RAID Disassembly button
                    if ui.button("🔧 Disassemble RAID").clicked() {
                        status_msg = Some("🔄 Starting RAID disassembly...".to_string());
                        match self.disassemble_existing_raids() {
                            Ok(result) => {
                                status_msg = Some(format!("✅ {}", result));
                            }
                            Err(e) => {
                                status_msg = Some(format!("❌ Error disassembling RAID: {}", e));
                                eprintln!("RAID disassembly error: {}", e);
                            }
                        }
                    }
                });
                
                // Update status message if needed
                if let Some(msg) = status_msg {
                    self.status = msg;
                }
                
                // Show GRUB config if available
                if let Some(show) = show_grub {
                    self.show_grub_config = show;
                }
                
                if let Some(config) = grub_config {
                    self.grub_config = config;
                }
                
                // Show GRUB config editor if requested
                if self.show_grub_config {
                    ui.separator();
                    ui.add_space(10.0);
                    ui.heading("GRUB Configuration");
                    
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.grub_config)
                                    .desired_rows(15)
                                    .desired_width(f32::INFINITY)
                                    .font(egui::TextStyle::Monospace)
                                    .code_editor()
                                    .interactive(false)
                            );
                        });
                        
                    // Add a button to close the GRUB config view
                    if ui.button("Close GRUB Config").clicked() {
                        self.show_grub_config = false;
                    }
                }
        });
    }
}

impl RaidCtlApp {
    /// Load existing GRUB configuration for pre-editing
    fn load_existing_grub_config(&mut self) {
        // Try to load the actual editable GRUB configuration file
        match std::fs::read_to_string("/etc/default/grub") {
            Ok(content) => {
                self.grub_config = content;
                self.status = "Loaded existing GRUB configuration from /etc/default/grub".to_string();
            }
            Err(e) => {
                self.status = format!("Warning: Could not load GRUB config: {}", e);
                // Create a basic GRUB config template
                self.grub_config = "# GRUB Configuration\nGRUB_DEFAULT=0\nGRUB_TIMEOUT=5\nGRUB_DISTRIBUTOR=`lsb_release -i -s 2> /dev/null || echo Debian`\nGRUB_CMDLINE_LINUX_DEFAULT=\"quiet splash\"\nGRUB_CMDLINE_LINUX=\"\"\n".to_string();
            }
        }
    }
    
    /// Disassemble existing RAID arrays
    fn disassemble_existing_raids(&mut self) -> Result<String> {
        use std::process::Command;
        
        // First, find existing RAID arrays
        let output = Command::new("cat")
            .arg("/proc/mdstat")
            .output()?;
            
        if !output.status.success() {
            return Err(anyhow::anyhow!("Failed to read /proc/mdstat"));
        }
        
        let mdstat_content = String::from_utf8_lossy(&output.stdout);
        let mut raid_devices = Vec::new();
        
        // Parse mdstat to find active RAID devices
        for line in mdstat_content.lines() {
            if line.starts_with("md") && line.contains("active") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(device) = parts.first() {
                    raid_devices.push(format!("/dev/{}", device));
                }
            }
        }
        
        if raid_devices.is_empty() {
            return Ok("No active RAID arrays found to disassemble".to_string());
        }
        
        let mut results = Vec::new();
        
        // Stop and disassemble each RAID array
        for device in &raid_devices {
            // Stop the array
            let stop_output = Command::new("mdadm")
                .args(&["--stop", device])
                .output()?;
                
            if stop_output.status.success() {
                results.push(format!("Stopped RAID array {}", device));
                
                // Zero the superblock on member devices
                let detail_output = Command::new("mdadm")
                    .args(&["--detail", device])
                    .output();
                    
                if let Ok(detail) = detail_output {
                    let detail_content = String::from_utf8_lossy(&detail.stdout);
                    for line in detail_content.lines() {
                        if line.contains("/dev/") && (line.contains("active sync") || line.contains("spare")) {
                            if let Some(dev_path) = line.split_whitespace().find(|s| s.starts_with("/dev/")) {
                                let zero_output = Command::new("mdadm")
                                    .args(&["--zero-superblock", dev_path])
                                    .output();
                                    
                                if zero_output.is_ok() {
                                    results.push(format!("Zeroed superblock on {}", dev_path));
                                }
                            }
                        }
                    }
                }
            } else {
                results.push(format!("Failed to stop RAID array {}: {}", 
                    device, String::from_utf8_lossy(&stop_output.stderr)));
            }
        }
        
        // Refresh device list after disassembly
        self.refresh_requested = true;
        
        Ok(format!("RAID disassembly completed:\n{}", results.join("\n")))
    }
    
    fn refresh_devices(&mut self, _ctx: &egui::Context) {
        self.status = "Refreshing devices...".to_string();
        
        // We'll run the device discovery in the main thread for simplicity
        // In a real application, you'd want to use a background thread
        // and proper message passing
        match Planner::discover_devices() {
            Ok(devices) => {
                let mut locked_devices = self.devices.lock().unwrap();
                *locked_devices = devices;
                self.status = format!("Found {} devices", locked_devices.len());
            }
            Err(e) => {
                self.status = format!("Error discovering devices: {}", e);
            }
        }
    }
    
    /// Save GRUB configuration to file with proper permissions
    fn save_grub_config(&self, config: &str) -> Result<()> {
        let path = "/boot/grub/grub.cfg";
        let mut file = File::create(path)?;
        
        // Write the configuration
        file.write_all(config.as_bytes())?;
        
        // Set permissions (readable by all, writable only by owner)
        let mut perms = file.metadata()?.permissions();
        perms.set_mode(0o644);  // rw-r--r--
        file.set_permissions(perms)?;
        
        Ok(())
    }
    
    /// Run update-grub command to apply changes
    fn run_update_grub(&self) -> Result<()> {
        let output = Command::new("update-grub")
            .output()?;
            
        if !output.status.success() {
            return Err(anyhow::anyhow!("Failed to update GRUB: {}", 
                String::from_utf8_lossy(&output.stderr)));
        }
        
        Ok(())
    }
    
    /// Comprehensive sanity check for the configuration
    fn verify_boot_configuration(&self, raid_level: &RaidLevel, devices: &[String]) -> Result<()> {
        // Check if any of the selected devices contain the current root filesystem
        let output = Command::new("findmnt")
            .arg("-n")
            .arg("-o")
            .arg("SOURCE")
            .arg("/")
            .output()?;
            
        if !output.status.success() {
            return Err(anyhow::anyhow!("Failed to determine root filesystem device"));
        }
        
        let root_device = String::from_utf8_lossy(&output.stdout).trim().to_string();
        
        // Check if the root device is in the list of selected devices
        if devices.iter().any(|d| root_device.contains(d)) {
            return Err(anyhow::anyhow!(
                "Cannot use the current root filesystem device ({}) in the RAID array. Please boot from a different device.",
                root_device
            ));
        }
        
        // Verify minimum disk requirements
        let min_disks = raid_level.min_disks();
        if devices.len() < min_disks {
            return Err(anyhow::anyhow!(
                "Insufficient disks: {} requires at least {} disks, but {} were selected",
                raid_level.display_name(), min_disks, devices.len()
            ));
        }
        
        // Check if devices exist and are not mounted
        for device in devices {
            // Check if device exists
            if !std::path::Path::new(device).exists() {
                return Err(anyhow::anyhow!("Device {} does not exist", device));
            }
            
            // Check if device is mounted
            let output = Command::new("findmnt")
                .arg("-n")
                .arg("-S")
                .arg(device)
                .output()?;
                
            if output.status.success() && !output.stdout.is_empty() {
                return Err(anyhow::anyhow!(
                    "Device {} is currently mounted. Please unmount it before using in RAID array", 
                    device
                ));
            }
        }
        
        // Check if mdadm is available
        if Command::new("which").arg("mdadm").output()?.status.success() == false {
            return Err(anyhow::anyhow!("mdadm is not installed or not in PATH"));
        }
        
        Ok(())
    }
    
    /// Apply the RAID configuration and update GRUB
    fn apply_raid_config(&mut self, raid_level: &RaidLevel, devices: &[String]) -> Result<()> {
        // Verify the configuration first
        self.verify_boot_configuration(raid_level, devices)?;
        
        // Backup GRUB configuration before making changes
        let backup_path = backup_grub_config()?;
        self.status = format!("GRUB configuration backed up to: {}", backup_path);
        
        // Generate the GRUB configuration
        let config = self.generate_grub_config(raid_level, devices)?;
        
        // Save the GRUB configuration
        self.save_grub_config(&config)?;
        
        // Run update-grub
        self.run_update_grub()?;
        
        // Update the in-memory GRUB config
        self.grub_config = config;
        
        Ok(())
    }
    
    /// Generate a GRUB configuration based on the selected RAID level and devices
    fn generate_grub_config(&mut self, raid_level: &RaidLevel, devices: &[String]) -> Result<String> {
        // Start with existing GRUB config as base
        let mut config = self.grub_config.clone();
        
        // Add RAID-specific configuration comments
        let raid_header = format!(
            "\n# RAID Provisioning Tool Configuration\n# RAID Level: {}\n# Devices: {}\n# Bootable: {}\n",
            raid_level.display_name(),
            devices.join(", "),
            if self.bootable_flag { "Yes" } else { "No" }
        );
        
        config.push_str(&raid_header);
        
        if self.bootable_flag {
            // Add RAID-specific GRUB settings for bootable arrays
            config.push_str("\n# RAID Boot Configuration\n");
            config.push_str("GRUB_PRELOAD_MODULES=\"mdraid09 mdraid1x\"\n");
            config.push_str(&format!("GRUB_CMDLINE_LINUX=\"$GRUB_CMDLINE_LINUX rd.auto rd.md.uuid={}\"\n", 
                self.generate_raid_uuid()));
            
            // Add device hints for GRUB
            config.push_str("\n# RAID Device Mapping\n");
            for (i, device) in devices.iter().enumerate() {
                config.push_str(&format!("# Device {}: {}\n", i, device));
            }
        } else {
            config.push_str("\n# RAID configured as non-bootable (data storage only)\n");
        }
        
        config.push_str("\n# End RAID Configuration\n");
        
        Ok(config)
    }
    
    /// Generate a placeholder RAID UUID for GRUB configuration
    fn generate_raid_uuid(&self) -> String {
        // In a real implementation, this would get the actual UUID from mdadm
        // For now, generate a placeholder
        "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx".to_string()
    }
}

fn main() -> Result<(), eframe::Error> {
    // Initialize logger
    env_logger::init();
    
    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(1000.0, 800.0)),
        min_window_size: Some(egui::vec2(800.0, 600.0)),
        ..Default::default()
    };
    
    eframe::run_native(
        "RAID Provisioning Tool",
        options,
        Box::new(|_cc| Box::new(RaidCtlApp::default())),
    )
}
