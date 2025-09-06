//! Core library for the RAID Provisioning Tool
//!
//! This crate provides the core functionality for discovering devices,
//! planning RAID configurations, and executing the provisioning process.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur during RAID provisioning
#[derive(Error, Debug)]
pub enum RaidError {
    #[error("Device not found: {0}")]
    DeviceNotFound(String),
    
    #[error("Invalid RAID level: {0}")]
    InvalidRaidLevel(String),
    
    #[error("Insufficient disks for RAID level {level}: required {required}, found {found}")]
    InsufficientDisks { level: String, required: usize, found: usize },
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Supported RAID levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum RaidLevel {
    /// No RAID (single disk)
    None,
    /// RAID 0 (striping)
    #[serde(rename = "raid0")]
    Raid0,
    /// RAID 1 (mirroring)
    #[serde(rename = "raid1")]
    Raid1,
    /// RAID 5 (striping with parity)
    #[serde(rename = "raid5")]
    Raid5,
    /// RAID 6 (striping with double parity)
    #[serde(rename = "raid6")]
    Raid6,
    /// RAID 10 (mirrored stripes)
    #[serde(rename = "raid10")]
    Raid10,
}

impl RaidLevel {
    /// Get the minimum number of disks required for this RAID level
    pub fn min_disks(&self) -> usize {
        match self {
            RaidLevel::None => 1,
            RaidLevel::Raid0 => 2,
            RaidLevel::Raid1 => 2,
            RaidLevel::Raid5 => 3,
            RaidLevel::Raid6 => 4,
            RaidLevel::Raid10 => 4,
        }
    }
    
    /// Get a list of all supported RAID levels
    pub fn all() -> Vec<Self> {
        vec![
            RaidLevel::None,
            RaidLevel::Raid0,
            RaidLevel::Raid1,
            RaidLevel::Raid5,
            RaidLevel::Raid6,
            RaidLevel::Raid10,
        ]
    }
    
    /// Get display name for the RAID level
    pub fn display_name(&self) -> &'static str {
        match self {
            RaidLevel::None => "None",
            RaidLevel::Raid0 => "RAID 0",
            RaidLevel::Raid1 => "RAID 1",
            RaidLevel::Raid5 => "RAID 5",
            RaidLevel::Raid6 => "RAID 6",
            RaidLevel::Raid10 => "RAID 10",
        }
    }
    
    /// Get short description for the RAID level (6 words max)
    pub fn description(&self) -> &'static str {
        match self {
            RaidLevel::None => "Single disk, no redundancy",
            RaidLevel::Raid0 => "Striping for performance, no redundancy",
            RaidLevel::Raid1 => "Mirroring for redundancy and performance",
            RaidLevel::Raid5 => "Striping with parity, fault tolerant",
            RaidLevel::Raid6 => "Striping with double parity protection",
            RaidLevel::Raid10 => "Mirrored stripes, high performance redundancy",
        }
    }
}

/// Device information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub path: String,
    pub size: u64,
    pub model: Option<String>,
    pub serial: Option<String>,
}

impl Device {
    /// Format the size in human-readable format
    pub fn format_size(&self) -> String {
        const KB: f64 = 1024.0;
        const MB: f64 = KB * 1024.0;
        const GB: f64 = MB * 1024.0;
        const TB: f64 = GB * 1024.0;
        
        let size = self.size as f64;
        if size >= TB {
            format!("{:.2} TB", size / TB)
        } else if size >= GB {
            format!("{:.2} GB", size / GB)
        } else if size >= MB {
            format!("{:.2} MB", size / MB)
        } else if size >= KB {
            format!("{:.2} KB", size / KB)
        } else {
            format!("{} B", size)
        }
    }
    
    /// Get a display name for the device
    pub fn display_name(&self) -> String {
        if let Some(model) = &self.model {
            format!("{} ({}) - {}", self.path, model, self.format_size())
        } else {
            format!("{} - {}", self.path, self.format_size())
        }
    }
}

/// Filesystem type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Filesystem {
    #[serde(rename = "ext4")]
    Ext4,
    #[serde(rename = "ext3")]
    Ext3,
    #[serde(rename = "ext2")]
    Ext2,
    #[serde(rename = "xfs")]
    Xfs,
    #[serde(rename = "btrfs")]
    Btrfs,
    #[serde(rename = "reiserfs")]
    ReiserFs,
    #[serde(rename = "jfs")]
    Jfs,
    #[serde(rename = "ntfs")]
    Ntfs,
    #[serde(rename = "fat32")]
    Fat32,
    #[serde(rename = "exfat")]
    ExFat,
}

impl Filesystem {
    /// Get the command to format a device with this filesystem
    pub fn format_command(&self, device: &str) -> Vec<String> {
        match self {
            Filesystem::Ext4 => vec!["mkfs.ext4".to_string(), "-F".to_string(), device.to_string()],
            Filesystem::Ext3 => vec!["mkfs.ext3".to_string(), "-F".to_string(), device.to_string()],
            Filesystem::Ext2 => vec!["mkfs.ext2".to_string(), "-F".to_string(), device.to_string()],
            Filesystem::Xfs => vec!["mkfs.xfs".to_string(), "-f".to_string(), device.to_string()],
            Filesystem::Btrfs => vec!["mkfs.btrfs".to_string(), "-f".to_string(), device.to_string()],
            Filesystem::ReiserFs => vec!["mkfs.reiserfs".to_string(), "-f".to_string(), device.to_string()],
            Filesystem::Jfs => vec!["mkfs.jfs".to_string(), "-f".to_string(), device.to_string()],
            Filesystem::Ntfs => vec!["mkfs.ntfs".to_string(), "-f".to_string(), device.to_string()],
            Filesystem::Fat32 => vec!["mkfs.fat".to_string(), "-F32".to_string(), device.to_string()],
            Filesystem::ExFat => vec!["mkfs.exfat".to_string(), device.to_string()],
        }
    }
    
    /// Get the display name for this filesystem
    pub fn display_name(&self) -> &'static str {
        match self {
            Filesystem::Ext4 => "ext4",
            Filesystem::Ext3 => "ext3", 
            Filesystem::Ext2 => "ext2",
            Filesystem::Xfs => "xfs",
            Filesystem::Btrfs => "btrfs",
            Filesystem::ReiserFs => "reiserfs",
            Filesystem::Jfs => "jfs",
            Filesystem::Ntfs => "ntfs",
            Filesystem::Fat32 => "fat32",
            Filesystem::ExFat => "exfat",
        }
    }
    
    /// Get short description for the filesystem (6 words max)
    pub fn description(&self) -> &'static str {
        match self {
            Filesystem::Ext4 => "Modern Linux journaling filesystem",
            Filesystem::Ext3 => "Legacy Linux journaling filesystem", 
            Filesystem::Ext2 => "Basic Linux filesystem, no journaling",
            Filesystem::Xfs => "High performance journaling filesystem",
            Filesystem::Btrfs => "Advanced filesystem with snapshots",
            Filesystem::ReiserFs => "Legacy journaling filesystem for Linux",
            Filesystem::Jfs => "IBM journaled filesystem for Linux",
            Filesystem::Ntfs => "Windows native filesystem format",
            Filesystem::Fat32 => "Universal compatibility filesystem format",
            Filesystem::ExFat => "Extended FAT for large files",
        }
    }
    
    /// Parse filesystem from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "ext4" => Some(Filesystem::Ext4),
            "ext3" => Some(Filesystem::Ext3),
            "ext2" => Some(Filesystem::Ext2),
            "xfs" => Some(Filesystem::Xfs),
            "btrfs" => Some(Filesystem::Btrfs),
            "reiserfs" => Some(Filesystem::ReiserFs),
            "jfs" => Some(Filesystem::Jfs),
            "ntfs" => Some(Filesystem::Ntfs),
            "fat32" => Some(Filesystem::Fat32),
            "exfat" => Some(Filesystem::ExFat),
            _ => None,
        }
    }
    
    /// Get all supported filesystem types
    pub fn all() -> Vec<Self> {
        vec![
            Filesystem::Ext4,
            Filesystem::Ext3,
            Filesystem::Ext2,
            Filesystem::Xfs,
            Filesystem::Btrfs,
            Filesystem::ReiserFs,
            Filesystem::Jfs,
            Filesystem::Ntfs,
            Filesystem::Fat32,
            Filesystem::ExFat,
        ]
    }
}

/// Configuration for the RAID provisioning tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub dry_run: bool,
    pub log_level: String,
    pub backup_existing_configs: bool,
    pub target_mount: String,
    pub grub_timeout: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            dry_run: true,
            log_level: "info".to_string(),
            backup_existing_configs: true,
            target_mount: "/target".to_string(),
            grub_timeout: 5,
        }
    }
}

/// RAID provisioning planner
pub struct Planner {
    devices: Vec<Device>,
    config: Config,
}

impl Planner {
    /// Create a new planner with the given devices and configuration
    pub fn new(devices: Vec<Device>, config: Config) -> Self {
        Self { devices, config }
    }
    
    /// Discover available block devices using lsblk
    pub fn discover_devices() -> Result<Vec<Device>> {
        use std::process::Command;
        use std::str;
        
        // Run lsblk to get device information in JSON format
        let output = Command::new("lsblk")
            .args(&["-J", "-o", "NAME,SIZE,MODEL,SERIAL,TYPE,MOUNTPOINT"])
            .output()?;
        
        if !output.status.success() {
            return Err(anyhow::anyhow!("Failed to run lsblk"));
        }
        
        // Parse the JSON output
        let json_str = str::from_utf8(&output.stdout)?;
        let lsblk_output: serde_json::Value = serde_json::from_str(json_str)?;
        
        let mut devices = Vec::new();
        
        // Extract block devices
        if let Some(blockdevices) = lsblk_output.get("blockdevices").and_then(|v| v.as_array()) {
            for device in blockdevices {
                // Skip devices that are mounted or not disks
                if device.get("mountpoint").and_then(|v| v.as_str()).is_some_and(|s| !s.is_empty()) {
                    continue;
                }
                
                if device.get("type").and_then(|v| v.as_str()) != Some("disk") {
                    continue;
                }
                
                // Extract device information
                let name = device.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let path = format!("/dev/{}", name);
                
                // Parse size (lsblk outputs size as a string like "800G")
                let size_str = device.get("size").and_then(|v| v.as_str()).unwrap_or("0");
                let size = parse_size(size_str);
                
                let model = device.get("model").and_then(|v| v.as_str()).map(|s| s.trim().to_string());
                let serial = device.get("serial").and_then(|v| v.as_str()).map(|s| s.trim().to_string());
                
                // Skip devices with 0 size
                if size == 0 {
                    continue;
                }
                
                devices.push(Device {
                    id: name.clone(),
                    path,
                    size,
                    model,
                    serial,
                });
            }
        }
        
        Ok(devices)
    }
    
    /// Plan a RAID configuration
    pub fn plan(&self, raid_level: RaidLevel, disks: &[String], filesystem: Option<Filesystem>) -> Result<ProvisioningPlan> {
        // Validate that we have enough disks for the requested RAID level
        let min_disks = raid_level.min_disks();
        
        if disks.len() < min_disks {
            return Err(RaidError::InsufficientDisks {
                level: format!("{:?}", raid_level),
                required: min_disks,
                found: disks.len(),
            }
            .into());
        }
        
        // Validate that all specified disks exist
        let mut valid_disks = Vec::new();
        for disk_path in disks {
            if !self.devices.iter().any(|d| &d.path == disk_path) {
                return Err(RaidError::DeviceNotFound(disk_path.clone()).into());
            }
            valid_disks.push(disk_path.clone());
        }
        
        // Create a provisioning plan
        Ok(ProvisioningPlan {
            raid_level,
            disks: valid_disks,
            filesystem: filesystem.unwrap_or(Filesystem::Ext4),
            mount_point: self.config.target_mount.clone(),
        })
    }
}

/// Helper function to parse size strings like "800G" into bytes
fn parse_size(size_str: &str) -> u64 {
    let size_str = size_str.trim();
    if size_str.is_empty() {
        return 0;
    }
    
    // Get the numeric part and unit
    let (num_str, unit) = size_str.chars()
        .position(|c| !c.is_ascii_digit() && c != '.')
        .map_or_else(
            || (size_str, ""),
            |pos| size_str.split_at(pos)
        );
        
    let num: f64 = num_str.parse().unwrap_or(0.0);
    let multiplier = match unit.to_uppercase().as_str() {
        "K" | "KB" => 1024.0,
        "M" | "MB" => 1024.0 * 1024.0,
        "G" | "GB" => 1024.0 * 1024.0 * 1024.0,
        "T" | "TB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => 1.0, // assume bytes if no unit
    };
    
    (num * multiplier) as u64
}

/// A provisioning plan that describes what will be done
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisioningPlan {
    pub raid_level: RaidLevel,
    pub disks: Vec<String>,
    pub filesystem: Filesystem,
    pub mount_point: String,
}

/// Execute a provisioning plan
pub fn execute_plan(plan: &ProvisioningPlan, config: &Config) -> Result<()> {
    use std::process::Command;
    
    if config.dry_run {
        log::info!("DRY RUN: Would execute plan: {:?}", plan);
        return Ok(());
    }
    
    log::info!("Executing plan: {:?}", plan);
    
    // Create RAID array using mdadm
    let raid_device = "/dev/md0"; // Default RAID device name
    
    // Build mdadm command
    let mut mdadm_cmd = vec![
        "mdadm".to_string(),
        "--create".to_string(),
        raid_device.to_string(),
        "--level".to_string(),
        match plan.raid_level {
            RaidLevel::None => "linear".to_string(),
            RaidLevel::Raid0 => "0".to_string(),
            RaidLevel::Raid1 => "1".to_string(),
            RaidLevel::Raid5 => "5".to_string(),
            RaidLevel::Raid6 => "6".to_string(),
            RaidLevel::Raid10 => "10".to_string(),
        },
        "--raid-devices".to_string(),
        plan.disks.len().to_string(),
    ];
    
    // Add device paths
    mdadm_cmd.extend(plan.disks.iter().cloned());
    
    // Execute mdadm command
    log::info!("Creating RAID array with command: {:?}", mdadm_cmd);
    let output = Command::new(&mdadm_cmd[0])
        .args(&mdadm_cmd[1..])
        .output()?;
    
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to create RAID array: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    
    // Format the RAID device with the selected filesystem
    let format_cmd = plan.filesystem.format_command(raid_device);
    log::info!("Formatting RAID array with command: {:?}", format_cmd);
    
    let output = Command::new(&format_cmd[0])
        .args(&format_cmd[1..])
        .output()?;
    
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to format RAID array: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    
    // Create mount point if it doesn't exist
    std::fs::create_dir_all(&plan.mount_point)?;
    
    // Mount the RAID array
    log::info!("Mounting RAID array to {}", plan.mount_point);
    let output = Command::new("mount")
        .args(&[raid_device, &plan.mount_point])
        .output()?;
    
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to mount RAID array: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    
    log::info!("RAID provisioning completed successfully");
    Ok(())
}
