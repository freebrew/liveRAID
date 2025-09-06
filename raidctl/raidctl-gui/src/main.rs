use eframe::egui;
use raidctl_core::{Device, Planner, RaidLevel, Filesystem};
use std::sync::{Arc, Mutex};
use anyhow::Result;

const FILESYSTEM_TYPES: &[&str] = &[
    "ext4", "ext3", "ext2", "xfs", "btrfs", 
    "reiserfs", "jfs", "ntfs", "fat32", "exfat"
];

pub struct RaidCtlApp {
    devices: Arc<Mutex<Vec<Device>>>,
    selected_devices: Vec<String>,
    selected_raid_level: Option<RaidLevel>,
    selected_filesystem: Option<String>,
    bootable_flag: bool,
    status: String,
    grub_config: String,
    show_grub_config: bool,
    refresh_requested: bool,
    current_plan: Option<(RaidLevel, Vec<String>, String, bool)>, // (raid_level, devices, filesystem, bootable)
    detected_raid_entries: Vec<RaidEntry>,
    is_live_environment: bool,
    available_tools: AvailableTools,
}

#[derive(Debug, Clone)]
struct AvailableTools {
    fdisk: bool,
    parted: bool,
    gparted: bool,
    gnome_disks: bool,
}

#[derive(Debug, Clone)]
struct RaidEntry {
    start_line: usize,
    end_line: usize,
    raid_level: String,
    filesystem: String,
    device_count: usize,
    header_comment: String,
}

impl Default for RaidCtlApp {
    fn default() -> Self {
        let mut app = Self {
            devices: Arc::new(Mutex::new(Vec::new())),
            selected_devices: Vec::new(),
            selected_raid_level: None,
            selected_filesystem: Some("ext4".to_string()),
            bootable_flag: false,
            status: "Ready".to_string(),
            grub_config: String::new(),
            show_grub_config: false,
            refresh_requested: true,
            current_plan: None,
            detected_raid_entries: Vec::new(),
            is_live_environment: Self::detect_live_environment(),
            available_tools: Self::detect_available_tools(),
        };
        app.load_existing_grub_config();
        app
    }
}

impl RaidCtlApp {
    fn detect_live_environment() -> bool {
        // Check for common live environment indicators
        std::path::Path::exists(std::path::Path::new("/run/live")) ||
        std::path::Path::exists(std::path::Path::new("/lib/live")) ||
        std::env::var("LIVE_MEDIA").is_ok() ||
        std::fs::read_to_string("/proc/cmdline")
            .map(|cmdline| cmdline.contains("boot=live") || cmdline.contains("live-media"))
            .unwrap_or(false)
    }

    fn detect_available_tools() -> AvailableTools {
        use std::process::Command;
        
        let check_command = |cmd: &str| -> bool {
            Command::new("which")
                .arg(cmd)
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false)
        };
        
        AvailableTools {
            fdisk: check_command("fdisk"),
            parted: check_command("parted"),
            gparted: check_command("gparted"),
            gnome_disks: check_command("gnome-disks"),
        }
    }

    fn parse_raid_entries(&mut self) {
        self.detected_raid_entries.clear();
        let lines: Vec<&str> = self.grub_config.lines().collect();
        
        let mut i = 0;
        while i < lines.len() {
            if let Some(line) = lines.get(i) {
                // Look for our template markers - more flexible matching
                if line.contains("Provision") && line.ends_with("#") && 
                   (line.contains("RAID") || line.contains("raid")) {
                    let header = line.to_string();
                    let start_line = i;
                    
                    // Parse the header to extract info - improved parsing
                    let parts: Vec<&str> = header.split_whitespace().collect();
                    let mut raid_level = "Unknown".to_string();
                    let mut filesystem = "Unknown".to_string();
                    let mut device_count = 0;
                    
                    // Find RAID level (look for RAID followed by number/letter)
                    for part in &parts {
                        if part.starts_with("RAID") || part.starts_with("raid") {
                            raid_level = part.to_string();
                        } else if part.len() >= 3 && (part.contains("EXT") || part.contains("BTRFS") || 
                                  part.contains("XFS") || part.contains("NTFS") || part.contains("FAT")) {
                            filesystem = part.to_string();
                        } else if part.ends_with("x") {
                            if let Some(num_str) = part.strip_suffix("x") {
                                device_count = num_str.parse().unwrap_or(0);
                            }
                        }
                    }
                    
                    // Find the end marker
                    let mut end_line = start_line;
                    for j in (i + 1)..lines.len() {
                        if let Some(end_line_content) = lines.get(j) {
                            if end_line_content.starts_with("# EOP") || end_line_content.starts_with("# End") {
                                end_line = j;
                                break;
                            }
                        }
                    }
                    
                    self.detected_raid_entries.push(RaidEntry {
                        start_line,
                        end_line,
                        raid_level,
                        filesystem,
                        device_count,
                        header_comment: header,
                    });
                    
                    i = end_line + 1;
                } else {
                    i += 1;
                }
            } else {
                break;
            }
        }
    }

    fn remove_raid_entry(&mut self, entry_index: usize) {
        if entry_index < self.detected_raid_entries.len() {
            // Clone the entry data to avoid borrowing issues
            let entry_header = self.detected_raid_entries[entry_index].header_comment.clone();
            let start_line = self.detected_raid_entries[entry_index].start_line;
            let end_line = self.detected_raid_entries[entry_index].end_line;
            
            let lines: Vec<&str> = self.grub_config.lines().collect();
            
            // Remove the entire block from start_line to end_line (inclusive)
            let mut new_lines = Vec::new();
            
            for (i, line) in lines.iter().enumerate() {
                if i < start_line || i > end_line {
                    new_lines.push(*line);
                }
            }
            
            // Remove extra blank lines that might be left
            while new_lines.len() > 1 && new_lines[new_lines.len() - 1].trim().is_empty() && new_lines[new_lines.len() - 2].trim().is_empty() {
                new_lines.pop();
            }
            
            self.grub_config = new_lines.join("\n");
            
            // Clear current plan state to allow new block creation
            self.current_plan = None;
            
            // Re-parse entries after removal
            self.parse_raid_entries();
            
            self.status = format!("Removed RAID entry: {}", entry_header);
        }
    }

    fn launch_partition_tool(&self, tool: &str) {
        use std::process::Command;
        
        let command = if self.is_live_environment {
            // In live environment, launch with appropriate terminal
            match tool {
                "fdisk" => vec!["x-terminal-emulator", "-e", "sudo", "fdisk", "-l"],
                "parted" => vec!["x-terminal-emulator", "-e", "sudo", "parted", "-l"],
                "gparted" => vec!["sudo", "gparted"],
                _ => return,
            }
        } else {
            // In installed system
            match tool {
                "fdisk" => vec!["gnome-terminal", "--", "sudo", "fdisk", "-l"],
                "parted" => vec!["gnome-terminal", "--", "sudo", "parted", "-l"],
                "gparted" => vec!["gparted"],
                _ => return,
            }
        };
        
        let _ = Command::new(&command[0])
            .args(&command[1..])
            .spawn();
    }

    fn write_to_fstab(&self) -> Result<()> {
        use std::fs::OpenOptions;
        use std::io::Write;
        
        // Generate fstab entry based on current RAID configuration
        if let Some((raid_level, devices, filesystem, _)) = &self.current_plan {
            let mount_point = format!("/mnt/raid_{}", raid_level.display_name().to_lowercase().replace(" ", ""));
            let device_path = "/dev/md0"; // Default RAID device
            let fs_type = filesystem.to_lowercase();
            let options = match fs_type.as_str() {
                "ext4" | "ext3" | "ext2" => "defaults",
                "xfs" => "defaults,noatime",
                "btrfs" => "defaults,compress=zstd",
                "ntfs" => "defaults,uid=1000,gid=1000",
                "fat32" => "defaults,uid=1000,gid=1000,umask=022",
                _ => "defaults",
            };
            
            let fstab_entry = format!(
                "\n# RAID {} Configuration - {} filesystem on {} devices\n{} {} {} {} 0 2\n",
                raid_level.display_name(),
                filesystem,
                devices.len(),
                device_path,
                mount_point,
                fs_type,
                options
            );
            
            // Create mount point directory
            std::fs::create_dir_all(&mount_point).ok();
            
            // Append to fstab
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open("/etc/fstab")?;
            file.write_all(fstab_entry.as_bytes())?;
            
            // Launch partition manager after writing fstab
            self.launch_partition_manager();
            
            Ok(())
        } else {
            Err(anyhow::anyhow!("No RAID configuration available to write to fstab"))
        }
    }
    
    fn launch_partition_manager(&self) {
        use std::process::Command;
        
        // Try GParted first, then gnome-disks
        if self.available_tools.gparted {
            let _ = if self.is_live_environment {
                Command::new("sudo").arg("gparted").spawn()
            } else {
                Command::new("gparted").spawn()
            };
        } else if self.available_tools.gnome_disks {
            let _ = Command::new("gnome-disks").spawn();
        }
    }

    fn create_new_plan(&mut self, raid_level: &RaidLevel, selected_devices: &[String], filesystem: &str, 
                       status_msg: &mut Option<String>, grub_config: &mut Option<String>, show_grub: &mut Option<bool>) {
        // Verify configuration before planning
        match self.verify_boot_configuration(&raid_level, &selected_devices) {
            Ok(_) => {
                let raid_level_clone = raid_level.clone();
                *status_msg = Some(format!("✅ Plan created for {} with {} devices using {} filesystem", 
                    raid_level.display_name(), 
                    selected_devices.len(),
                    filesystem));
                
                // Store the current plan to prevent duplicates
                self.current_plan = Some((raid_level_clone.clone(), selected_devices.to_vec(), filesystem.to_string(), self.bootable_flag));
                
                // Generate a preview of the GRUB config and RAID script
                match self.generate_grub_config(&raid_level_clone, &selected_devices) {
                    Ok(config) => {
                        *grub_config = Some(config);
                        *show_grub = Some(true);
                        
                        // Also generate and save the executable RAID script
                        if let Ok(script) = self.generate_raid_script(&raid_level_clone, &selected_devices) {
                            let script_path = format!("/tmp/raid_setup_{}.sh", raid_level_clone.display_name().to_lowercase().replace(" ", "_"));
                            if let Err(e) = std::fs::write(&script_path, &script) {
                                eprintln!("Warning: Could not save RAID script: {}", e);
                            } else {
                                // Make script executable
                                use std::process::Command;
                                let _ = Command::new("chmod").arg("+x").arg(&script_path).output();
                                *status_msg = Some(format!("✅ Plan created. RAID script saved to {}", script_path));
                            }
                        }
                    }
                    Err(e) => {
                        *status_msg = Some(format!("❌ Error generating GRUB config: {}", e));
                        eprintln!("GRUB config generation error: {}", e);
                    }
                }
            }
            Err(e) => {
                *status_msg = Some(format!("❌ Configuration error: {}", e));
                eprintln!("Configuration verification error: {}", e);
            }
        }
    }
}

impl eframe::App for RaidCtlApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.refresh_requested {
            self.refresh_devices(ctx);
            self.refresh_requested = false;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("RAID Provisioning Tool");
            ui.add_space(10.0);

            // Status bar with refresh button
            ui.horizontal(|ui| {
                if ui.button("🔄 Refresh").clicked() {
                    self.refresh_requested = true;
                    self.load_existing_grub_config();
                    self.parse_raid_entries();
                    self.available_tools = Self::detect_available_tools();
                    self.status = "Device list and tools refreshed".to_string();
                }
            });

            ui.separator();
            ui.add_space(10.0);

            let selected_devices = self.selected_devices.clone();
            let devices_clone = {
                let devices = self.devices.lock().unwrap();
                devices.clone()
            };

            // Main device grid with RAID level and filesystem selection at the top
            ui.vertical(|ui| {
                // RAID Level and Filesystem Type in one row
                ui.horizontal(|ui| {
                    // RAID Level dropdown
                    ui.vertical(|ui| {
                        ui.heading("RAID Level");
                        
                        let selected_text = if let Some(level) = &self.selected_raid_level {
                            format!("{} - {}", level.display_name(), level.description())
                        } else {
                            "Select RAID Level".to_string()
                        };
                        
                        egui::ComboBox::from_id_source("raid_level_combo")
                            .selected_text(selected_text)
                            .width(300.0)
                            .show_ui(ui, |ui| {
                                for level in RaidLevel::all().iter() {
                                    let text = format!("{} - {}", level.display_name(), level.description());
                                    let is_selected = self.selected_raid_level.as_ref() == Some(level);
                                    if ui.selectable_label(is_selected, text).clicked() {
                                        self.selected_raid_level = Some(level.clone());
                                        self.current_plan = None; // Clear plan when RAID level changes
                                    }
                                }
                            });
                    });
                    
                    ui.add_space(20.0);
                    
                    // Filesystem Type dropdown
                    ui.vertical(|ui| {
                        ui.heading("Filesystem Type");
                        
                        let selected_text = if let Some(fs_name) = &self.selected_filesystem {
                            if let Some(filesystem) = Filesystem::from_str(fs_name) {
                                format!("{} - {}", filesystem.display_name(), filesystem.description())
                            } else {
                                fs_name.clone()
                            }
                        } else {
                            "Select Filesystem Type".to_string()
                        };
                        
                        egui::ComboBox::from_id_source("filesystem_combo")
                            .selected_text(selected_text)
                            .width(300.0)
                            .show_ui(ui, |ui| {
                                for fs_type in FILESYSTEM_TYPES.iter() {
                                    if let Some(filesystem) = Filesystem::from_str(fs_type) {
                                        let text = format!("{} - {}", filesystem.display_name(), filesystem.description());
                                        let is_selected = self.selected_filesystem.as_ref() == Some(&fs_type.to_string());
                                        if ui.selectable_label(is_selected, text).clicked() {
                                            self.selected_filesystem = Some(fs_type.to_string());
                                            self.current_plan = None; // Clear plan when filesystem changes
                                        }
                                    }
                                }
                            });
                    });
                });

                ui.separator();
                ui.add_space(10.0);

                // Boot flag checkbox
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut self.bootable_flag, "Mark RAID as bootable").clicked() {
                        self.current_plan = None; // Clear plan when bootable flag changes
                    }
                });
                
                ui.separator();
                ui.add_space(10.0);
                
                // Device selection
                ui.heading("Available Storage Devices");
                
                if devices_clone.is_empty() {
                    ui.label("No storage devices found. Click 'Refresh Devices' to scan.");
                } else {
                    // Get the selection color before creating the grid
                    let _selection_color = ui.style().visuals.selection.bg_fill;
                    
                    egui::Grid::new("devices_grid")
                        .num_columns(2)
                        .spacing([20.0, 10.0])
                        .show(ui, |ui| {
                            for device in devices_clone.iter() {
                                let is_selected = self.selected_devices.contains(&device.path);
                                
                                // Device icon and selection button
                                ui.horizontal(|ui| {
                                    // Device icon that changes color when selected
                                    let icon_color = if is_selected {
                                        egui::Color32::from_rgb(0, 200, 100) // Green when selected
                                    } else {
                                        egui::Color32::from_rgb(100, 150, 200) // Blue when not selected
                                    };
                                    
                                    ui.colored_label(icon_color, "💾");
                                    
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
                                            self.selected_devices.retain(|d| d != &device.path);
                                        } else {
                                            self.selected_devices.push(device.path.clone());
                                        }
                                        self.current_plan = None; // Clear plan when device selection changes
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
                        if let Some(raid_level) = self.selected_raid_level.clone() {
                            if !selected_devices.is_empty() {
                                let filesystem = self.selected_filesystem.clone().unwrap_or_else(|| "ext4".to_string());
                                let new_plan = (raid_level.clone(), selected_devices.clone(), filesystem.clone(), self.bootable_flag);
                                
                                // Check if this plan is identical to the current plan
                                if let Some(ref current) = self.current_plan {
                                    if current.0 == new_plan.0 && current.1 == new_plan.1 && current.2 == new_plan.2 && current.3 == new_plan.3 {
                                        status_msg = Some("ℹ️ Plan already exists with identical configuration".to_string());
                                    } else {
                                        // Different plan, proceed with creation
                                        self.create_new_plan(&raid_level, &selected_devices, &filesystem, &mut status_msg, &mut grub_config, &mut show_grub);
                                    }
                                } else {
                                    // No current plan, create new one
                                    self.create_new_plan(&raid_level, &selected_devices, &filesystem, &mut status_msg, &mut grub_config, &mut show_grub);
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
                                        let _filesystem = self.selected_filesystem.clone().unwrap_or_else(|| "ext4".to_string());
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
            });
            
            // System Tools Section (moved above GRUB config)
            ui.separator();
            ui.add_space(10.0);
            ui.heading("System Tools");
            
            // Environment indicator
            let env_text = if self.is_live_environment {
                "🔴 Live Environment Detected"
            } else {
                "🟢 Installed System"
            };
            ui.label(env_text);
            ui.add_space(5.0);
            
            // Tool buttons
            ui.horizontal(|ui| {
                if self.available_tools.fdisk {
                    if ui.button("Launch fdisk").clicked() {
                        self.launch_partition_tool("fdisk");
                    }
                } else {
                    ui.add_enabled(false, egui::Button::new("Launch fdisk (not available)"));
                }
                
                if self.available_tools.parted {
                    if ui.button("Launch parted").clicked() {
                        self.launch_partition_tool("parted");
                    }
                } else {
                    ui.add_enabled(false, egui::Button::new("Launch parted (not available)"));
                }
                
                if self.available_tools.gparted {
                    if ui.button("Launch GParted").clicked() {
                        self.launch_partition_tool("gparted");
                    }
                } else {
                    ui.add_enabled(false, egui::Button::new("Launch GParted (not available)"));
                }
            });
            
            ui.horizontal(|ui| {
                if self.current_plan.is_some() {
                    if ui.button("Write To fstab").clicked() {
                        match self.write_to_fstab() {
                            Ok(_) => {
                                self.status = "✅ RAID configuration written to fstab successfully".to_string();
                            }
                            Err(e) => {
                                self.status = format!("❌ Error writing to fstab: {}", e);
                            }
                        }
                    }
                } else {
                    ui.add_enabled(false, egui::Button::new("Write To fstab (create plan first)"));
                }
            });
            
            // Show GRUB config editor if requested (moved below system tools)
            if self.show_grub_config {
                ui.separator();
                ui.add_space(10.0);
                ui.heading("GRUB Configuration");
                
                // Re-parse RAID entries every time GRUB config is shown to catch new entries
                self.parse_raid_entries();
                
                // Show detected RAID entries with removal buttons
                if !self.detected_raid_entries.is_empty() {
                    ui.separator();
                    ui.heading("Detected RAID Entries");
                    
                    let mut entries_to_remove = Vec::new();
                    for (i, entry) in self.detected_raid_entries.iter().enumerate() {
                        ui.horizontal(|ui| {
                            // Red X button to remove entry
                            if ui.add(egui::Button::new("❌").fill(egui::Color32::from_rgb(200, 50, 50))).clicked() {
                                entries_to_remove.push(i);
                            }
                            
                            ui.label(format!("{} {} ({}x drives)", 
                                entry.raid_level, 
                                entry.filesystem, 
                                entry.device_count));
                        });
                    }
                    
                    // Remove entries (in reverse order to maintain indices)
                    for &index in entries_to_remove.iter().rev() {
                        self.remove_raid_entry(index);
                    }
                    
                    ui.add_space(10.0);
                }
                
                // Calculate dynamic height based on screen height and content
                let screen_height = ctx.screen_rect().height();
                let max_text_height = screen_height * 0.6; // Use 60% of screen height max
                let content_lines = self.grub_config.lines().count();
                let line_height = 18.0; // Slightly larger line height for better readability
                let content_height = (content_lines as f32 * line_height).max(300.0);
                let text_area_height = content_height.min(max_text_height);
                
                // Use ScrollArea for automatic scrolling when content exceeds height
                egui::ScrollArea::vertical()
                    .max_height(text_area_height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.add_sized(
                            [ui.available_width(), content_height],
                            egui::TextEdit::multiline(&mut self.grub_config)
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
                self.parse_raid_entries(); // Parse existing RAID entries
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
    
    fn refresh_devices(&mut self, ctx: &egui::Context) {
        let devices_arc = Arc::clone(&self.devices);
        let ctx_clone = ctx.clone();
        
        // Also reload GRUB config when refreshing
        self.load_existing_grub_config();
        
        std::thread::spawn(move || {
            match raidctl_core::Planner::discover_devices() {
                Ok(new_devices) => {
                    let mut devices = devices_arc.lock().unwrap();
                    *devices = new_devices;
                    ctx_clone.request_repaint();
                }
                Err(e) => {
                    eprintln!("Error scanning devices: {}", e);
                }
            }
        });
    }

    fn apply_raid_config(&mut self, raid_level: &RaidLevel, devices: &[String]) -> Result<()> {
        // Verify configuration first
        self.verify_boot_configuration(raid_level, devices)?;
        
        // Create backup of GRUB config
        self.backup_grub_config()?;
        
        // Execute the RAID plan using the core library
        let filesystem_str = self.selected_filesystem.as_ref().map(|s| s.as_str()).unwrap_or("ext4");
        let filesystem = raidctl_core::Filesystem::from_str(filesystem_str)
            .ok_or_else(|| anyhow::anyhow!("Invalid filesystem type: {}", filesystem_str))?;
        
        // Get current devices for planner
        let current_devices = {
            let devices_lock = self.devices.lock().unwrap();
            devices_lock.clone()
        };
        
        let config = raidctl_core::Config::default();
        let planner = Planner::new(current_devices, config.clone());
        let plan = planner.plan(raid_level.clone(), devices, Some(filesystem))?;
        
        // Execute the plan using the core library's execute_plan method
        raidctl_core::execute_plan(&plan, &config)?;
        
        // Update GRUB configuration
        let grub_config = self.generate_grub_config(raid_level, devices)?;
        
        // Write the GRUB config to file
        std::fs::write("/etc/default/grub", &grub_config)?;
        
        // Run update-grub
        self.run_update_grub()?;
        
        // Update the in-memory GRUB config
        self.grub_config = grub_config;
        
        Ok(())
    }
    
    /// Generate device UUIDs for RAID configuration
    fn get_device_uuids(&self, devices: &[String]) -> Result<Vec<String>> {
        use std::process::Command;
        let mut uuids = Vec::new();
        
        for device in devices {
            let output = Command::new("blkid")
                .arg("-s")
                .arg("UUID")
                .arg("-o")
                .arg("value")
                .arg(device)
                .output()?;
                
            if output.status.success() {
                let uuid = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !uuid.is_empty() {
                    uuids.push(format!("UUID={}", uuid));
                } else {
                    // Fallback to device path if no UUID
                    uuids.push(device.clone());
                }
            } else {
                // Fallback to device path if blkid fails
                uuids.push(device.clone());
            }
        }
        
        Ok(uuids)
    }

    /// Generate a GRUB configuration based on the selected RAID level and devices
    fn generate_grub_config(&mut self, raid_level: &RaidLevel, devices: &[String]) -> Result<String> {
        // Always generate a minimal, live /etc/default/grub config for bootable RAID
        let mut config = String::new();
        if self.bootable_flag {
            let raid_uuid = self.generate_raid_uuid();
            let current_cmdline = self.extract_grub_cmdline();
            let new_cmdline = if current_cmdline.is_empty() {
                format!("rd.md.uuid={}", raid_uuid)
            } else {
                format!("{} rd.md.uuid={}", current_cmdline, raid_uuid)
            };
            config.push_str(&format!("GRUB_CMDLINE_LINUX=\"{}\"\n", new_cmdline));
            config.push_str("GRUB_PRELOAD_MODULES=\"mdraid09 mdraid1x\"\n");
            config.push_str("GRUB_TIMEOUT=5\n");
            config.push_str("GRUB_DEFAULT=0\n");
            config.push_str("GRUB_DISTRIBUTOR=\"RAID Provision\"\n");
        }
        Ok(config)
    }
    
    /// Extract current GRUB_CMDLINE_LINUX value from existing config
    fn extract_grub_cmdline(&self) -> String {
        for line in self.grub_config.lines() {
            if line.starts_with("GRUB_CMDLINE_LINUX=") {
                // Extract the value between quotes
                if let Some(start) = line.find('"') {
                    if let Some(end) = line.rfind('"') {
                        if start < end {
                            return line[start + 1..end].to_string();
                        }
                    }
                }
            }
        }
        // Default values commonly found in GRUB configurations
        "rhgb quiet".to_string()
    }
    
    /// Generate a separate executable script for RAID setup
    fn generate_raid_script(&mut self, raid_level: &RaidLevel, devices: &[String]) -> Result<String> {
        let device_uuids = self.get_device_uuids(devices).unwrap_or_else(|_| devices.to_vec());
        let filesystem = self.selected_filesystem.as_ref().map(|s| s.as_str()).unwrap_or("ext4").to_lowercase();
        
        let raid_level_str = match raid_level {
            raidctl_core::RaidLevel::Raid0 => "0",
            raidctl_core::RaidLevel::Raid1 => "1", 
            raidctl_core::RaidLevel::Raid5 => "5",
            raidctl_core::RaidLevel::Raid6 => "6",
            raidctl_core::RaidLevel::Raid10 => "10",
            _ => "0",
        };
        
        let mut script = String::new();
        script.push_str("#!/bin/bash\n");
        script.push_str("# RAID Setup Script\n");
        script.push_str("# Generated by RAID Provisioning Tool\n\n");
        script.push_str("set -e\n\n");
        
        script.push_str(&format!("echo \"Creating RAID {} array...\"\n", raid_level.display_name()));
        script.push_str(&format!("mdadm --create /dev/md0 --level={} --raid-devices={} {}\n\n", 
            raid_level_str, 
            devices.len(), 
            device_uuids.join(" ")));
            
        script.push_str("echo \"Waiting for RAID array to initialize...\"\n");
        script.push_str("sleep 5\n\n");
        
        script.push_str(&format!("echo \"Creating {} filesystem...\"\n", filesystem));
        script.push_str(&format!("mkfs.{} /dev/md0\n\n", filesystem));
        
        script.push_str("echo \"Creating mount point...\"\n");
        script.push_str("mkdir -p /mnt/raid\n\n");
        
        script.push_str("echo \"Mounting RAID array...\"\n");
        script.push_str("mount /dev/md0 /mnt/raid\n\n");
        
        script.push_str("echo \"Adding to fstab...\"\n");
        script.push_str(&format!("echo '/dev/md0 /mnt/raid {} defaults 0 2' >> /etc/fstab\n\n", filesystem));
        
        script.push_str("echo \"RAID setup completed successfully!\"\n");
        script.push_str("echo \"RAID array mounted at /mnt/raid\"\n");
        
        Ok(script)
    }
    
    /// Generate a placeholder RAID UUID for GRUB configuration
    fn generate_raid_uuid(&self) -> String {
        // In a real implementation, this would get the actual UUID from mdadm
        // For now, generate a placeholder
        "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx".to_string()
    }

    fn backup_grub_config(&self) -> Result<()> {
        use std::process::Command;
        use chrono::Utc;
        
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let backup_path = format!("/etc/default/grub.backup.{}", timestamp);
        
        let output = Command::new("cp")
            .args(&["/etc/default/grub", &backup_path])
            .output()?;
            
        if !output.status.success() {
            return Err(anyhow::anyhow!("Failed to backup GRUB config: {}", 
                String::from_utf8_lossy(&output.stderr)));
        }
        
        Ok(())
    }

    fn run_update_grub(&self) -> Result<()> {
        use std::process::Command;
        
        let output = Command::new("update-grub")
            .output()?;
            
        if !output.status.success() {
            return Err(anyhow::anyhow!("Failed to update GRUB: {}", 
                String::from_utf8_lossy(&output.stderr)));
        }
        
        Ok(())
    }

    fn verify_boot_configuration(&self, raid_level: &RaidLevel, devices: &[String]) -> Result<()> {
        use std::process::Command;
        
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
        if devices.iter().any(|d| root_device.contains(d)) {
            return Err(anyhow::anyhow!(
                "Cannot use the current root filesystem device ({}) in the RAID array. Please boot from a different device.",
                root_device
            ));
        }
        let min_disks = raid_level.min_disks();
        if devices.len() < min_disks {
            return Err(anyhow::anyhow!(
                "Insufficient disks: {} requires at least {} disks, but {} were selected",
                raid_level.display_name(), min_disks, devices.len()
            ));
        }
        for device in devices {
            if !std::path::Path::new(device).exists() {
                return Err(anyhow::anyhow!("Device {} does not exist", device));
            }
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
        if Command::new("which").arg("mdadm").output()?.status.success() == false {
            return Err(anyhow::anyhow!("mdadm is not installed or not in PATH"));
        }
        Ok(())
    }
}

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
        format!("{:.1} {}", size, UNITS[unit_index])
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
