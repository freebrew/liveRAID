//! Command-line interface for the RAID Provisioning Tool

use anyhow::Result;
use clap::{Parser, Subcommand};
use raidctl_core::{
    execute_plan, Config, Filesystem, Planner, ProvisioningPlan, RaidLevel,
};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    
    /// Enable verbose logging
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
    
    /// Configuration file path
    #[arg(short, long, default_value = "/etc/raidctl/config.toml")]
    config: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Discover available devices
    Discover,
    
    /// Plan a RAID configuration
    Plan {
        /// RAID level to use
        #[arg(short, long, value_enum)]
        level: RaidLevelCli,
        
        /// Disks to use (by path)
        #[arg(required = true)]
        disks: Vec<String>,
        
        /// Perform a dry run (don't actually make changes)
        #[arg(long, default_value = "true")]
        dry_run: bool,
    },
    
    /// Execute a provisioning plan
    Apply {
        /// Path to the plan file
        plan_file: String,
    },
}

/// CLI representation of RAID levels
#[derive(clap::ValueEnum, Clone, Debug)]
pub enum RaidLevelCli {
    None,
    Raid0,
    Raid1,
    Raid5,
    Raid6,
    Raid10,
}

impl From<RaidLevelCli> for RaidLevel {
    fn from(level: RaidLevelCli) -> Self {
        match level {
            RaidLevelCli::None => RaidLevel::None,
            RaidLevelCli::Raid0 => RaidLevel::Raid0,
            RaidLevelCli::Raid1 => RaidLevel::Raid1,
            RaidLevelCli::Raid5 => RaidLevel::Raid5,
            RaidLevelCli::Raid6 => RaidLevel::Raid6,
            RaidLevelCli::Raid10 => RaidLevel::Raid10,
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Initialize logger
    init_logger(cli.verbose);
    
    // Load configuration
    let config = load_config(&cli.config)?;
    
    match &cli.command {
        Commands::Discover => {
            let devices = Planner::discover_devices()?;
            println!("Discovered {} devices:", devices.len());
            for device in devices {
                println!("  {} ({})", device.path, device.id);
                if let Some(model) = &device.model {
                    println!("    Model: {}", model);
                }
                println!("    Size: {} bytes", device.size);
            }
        }
        Commands::Plan { level, disks, dry_run } => {
            let devices = Planner::discover_devices()?;
            let planner = Planner::new(devices, config);
            let raid_level = level.clone().into();
            let plan = planner.plan(raid_level, disks, Some(Filesystem::Ext4))?;
            
            if *dry_run {
                println!("Plan (dry run): {:#?}", plan);
            } else {
                println!("Plan: {:#?}", plan);
                // In a real implementation, we would save the plan to a file
            }
        }
        Commands::Apply { plan_file: _ } => {
            // In a real implementation, we would load the plan from the file
            // For this example, we'll create a dummy plan
            let plan = ProvisioningPlan {
                raid_level: RaidLevel::Raid1,
                disks: vec!["/dev/sda".to_string(), "/dev/sdb".to_string()],
                filesystem: Filesystem::Ext4,
                mount_point: "/target".to_string(),
            };
            
            execute_plan(&plan, &config)?;
            println!("Provisioning completed successfully!");
        }
    }
    
    Ok(())
}

/// Initialize the logger based on verbosity level
fn init_logger(verbosity: u8) {
    let level = match verbosity {
        0 => log::LevelFilter::Info,
        1 => log::LevelFilter::Debug,
        _ => log::LevelFilter::Trace,
    };
    
    env_logger::Builder::new()
        .filter_level(level)
        .init();
}

/// Load configuration from file
fn load_config(_path: &str) -> Result<Config> {
    // In a real implementation, we would load the config from a file
    // For this example, we'll return the default config
    Ok(Config::default())
}

