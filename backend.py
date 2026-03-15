import subprocess
import json
import time

# Set to False to actually execute formatting on physical disks!
DRY_RUN = False

def run_command(cmd_list, dry_run=None):
    if dry_run is None:
        dry_run = DRY_RUN
    
    cmd_str = " ".join(cmd_list)
    if dry_run:
        return True, f"[DRY RUN] Would execute: {cmd_str}\n"

    try:
        result = subprocess.run(cmd_list, capture_output=True, text=True, check=True)
        return True, (result.stdout + result.stderr).strip() + "\n"
    except subprocess.CalledProcessError as e:
        return False, f"Command failed: {cmd_str}\nError: {e.stderr}\n"
    except Exception as e:
        return False, f"Execution failed: {e}\n"

def get_used_raid_drives():
    drives = set()
    try:
        with open('/proc/mdstat', 'r') as f:
            for line in f:
                if line.startswith('md') and ':' in line:
                    parts = line.split()
                    for p in parts[3:]:  # the devices come after 'active raid1' etc e.g., sda[0]
                        if '[' in p:
                            dev_name = p.split('[')[0]
                            drives.add(dev_name)
    except Exception:
        pass
    return drives

def get_available_drives():
    """
    Returns a list of dictionaries with 'name' and 'size_gb' for unmounted block devices.
    Uses lsblk JSON output parsing. Filters out drives that are actively in a RAID array.
    """
    try:
        # Fetch block devices in JSON format, excluding loop devices (which LiveCDs use heavily)
        result = subprocess.run(['lsblk', '-J', '-b', '-o', 'NAME,TYPE,SIZE,MOUNTPOINTS'], capture_output=True, text=True, check=True)
        data = json.loads(result.stdout)
        
        used_raid_drives = get_used_raid_drives()
        
        drives = []
        for block_device in data.get('blockdevices', []):
            if block_device.get('type') == 'disk':
                name = block_device.get('name')
                
                # Exclude if it is part of a RAID
                if name in used_raid_drives:
                    continue
                
                size = int(block_device.get('size', 0))
                # For lsblk JSON parsing, 'mountpoints' can be [None] or unpopulated
                mountpoints = block_device.get('mountpoints', [])
                mountpoints = [m for m in mountpoints if m is not None]
                
                # Heuristics for blank, unmounted hard drives
                if not name.startswith('loop') and not name.startswith('md') and not mountpoints and size > 0:
                    drives.append({
                        'name': f"/dev/{name}",
                        'size_gb': round(size / (1024**3), 2)
                    })
        return drives
    except Exception as e:
        print(f"Error fetching physical drives: {e}")
        return []

def get_active_arrays():
    """
    Parses /proc/mdstat to return a list of currently running md devices.
    Returns: [{'name': '/dev/md0', 'type': 'raid1', 'status': 'active'}]
    """
    arrays = []
    try:
        with open('/proc/mdstat', 'r') as f:
            lines = f.readlines()
            
        for line in lines:
            line = line.strip()
            # Look for lines like: md0 : active raid1 sdb[1] sda[0]
            if line.startswith("md") and ":" in line:
                parts = line.split(":")
                md_name = parts[0].strip()
                details = parts[1].strip().split()
                if len(details) >= 2:
                    status = details[0] # active or inactive
                    raid_type = details[1]
                    arrays.append({
                        'name': f"/dev/{md_name}",
                        'status': status,
                        'type': raid_type
                    })
        return arrays
    except Exception as e:
        print(f"Error parsing /proc/mdstat: {e}")
        return []

def delete_raid(array_name):
    """
    Stops a running RAID array and zeroes the superblocks of its constituent devices.
    """
    logs = []
    
    # 1. Identify constituent devices before stopping
    success, detail_out = run_command(["mdadm", "--detail", array_name])
    if not success:
        return False, f"Failed to get details for {array_name}.\n{detail_out}\n"
    
    devices_to_zero = []
    for line in detail_out.split('\n'):
        if "/dev/sd" in line or "/dev/nvme" in line or "/dev/hd" in line:
            parts = line.split()
            device_path = parts[-1]
            if device_path.startswith("/dev/"):
                devices_to_zero.append(device_path)
                
    # 2. Unmount just in case (we ignore errors if it wasn't mounted)
    run_command(["umount", "-f", array_name], dry_run=False) # Always attempt real umount if needed, but not critical to logs usually
    
    # 3. Stop the array
    success, stop_out = run_command(["mdadm", "--stop", array_name])
    logs.append(stop_out)
    if not success:
        return False, "".join(logs)
        
    # 4. Zero the superblocks of the raw drives so they appear "blank" again
    for dev in devices_to_zero:
        success, zero_out = run_command(["mdadm", "--zero-superblock", dev])
        logs.append(zero_out)
        # Attempt to wipe thoroughly so lsblk updates instantly
        run_command(["wipefs", "-a", dev])
        
    # Flush udev so lsblk reflects the changes immediately
    run_command(["udevadm", "settle", "--timeout=2"])
        
    return True, "".join(logs)
    
def create_raid(level, device_paths, array_name="/dev/md0", chunk_size="Default", ssd_mode=False):
    num_devices = len(device_paths)
    if num_devices == 0:
        return False, "No devices selected for RAID.\n"
    
    cmd = [
        "mdadm", "--create", "--verbose", "--run", array_name,
        f"--level={level}", f"--raid-devices={num_devices}"
    ]
    
    if chunk_size != "Default":
        # Parse '64K' into '64'
        chunk_kb = chunk_size.replace("K", "")
        cmd.extend(["--chunk", chunk_kb])
        
    if ssd_mode:
        cmd.append("--assume-clean")
        
    cmd.extend(device_paths)
    
    return run_command(cmd)

def format_device(device_path, fs_type="ext4", boot_flag=False, trim_discard=False):
    logs = []
    
    # 1. Create a fresh GPT partition table
    success, out = run_command(["parted", "-s", device_path, "mklabel", "gpt"])
    logs.append(out)
    if not success: return False, "".join(logs)
    
    # 2. Create the primary partition using 100% of space
    success, out = run_command(["parted", "-s", device_path, "mkpart", "primary", "0%", "100%"])
    logs.append(out)
    if not success: return False, "".join(logs)
    
    # Give the system a second to register the partition table before tagging or formatting
    if not DRY_RUN:
        time.sleep(1)
        
    partition_path = f"{device_path}p1"
    
    # 3. Apply boot flag if requested
    if boot_flag:
        success, out = run_command(["parted", "-s", device_path, "set", "1", "boot", "on"])
        logs.append(out)
        if not success: return False, "".join(logs)
        
    # 4. Format the partition
    mkfs_cmd = ["mkfs.ext4"] # Default
    
    # Discard/TRIM flags differ by filesystem. e2fsprogs enables it by default usually, but we can force it.
    discard_flag = []
    if trim_discard:
        if fs_type in ["ext4"]:
            discard_flag = ["-E", "nodiscard=0"]
        elif fs_type in ["xfs"]:
            discard_flag = ["-K"] # Do not discard if false, default is usually true, but -K disables it. We want it enabled, so skip -K if true.
            pass
            
    if fs_type == "ext4":
        mkfs_cmd = ["mkfs.ext4"] + discard_flag
    elif fs_type == "btrfs":
        mkfs_cmd = ["mkfs.btrfs", "-f"]
    elif fs_type == "vfat":
        mkfs_cmd = ["mkfs.vfat"]
    elif fs_type == "xfs":
        # XFS enables discard natively, we only use -K to disable it if requested
        mkfs_cmd = ["mkfs.xfs", "-f"]
        if not trim_discard:
             mkfs_cmd.append("-K")
    elif fs_type == "zfs":
        # ZFS is not created via mkfs.*, it's a zpool creation command which would replace mdadm entirely.
        return False, "ERROR: ZFS unsupported in this GUI design as it handles its own RAID (RAID-Z).\n"
    elif fs_type == "f2fs":
        mkfs_cmd = ["mkfs.f2fs", "-f"]
    elif fs_type == "exfat":
        mkfs_cmd = ["mkfs.exfat"]
    elif fs_type == "ntfs":
        mkfs_cmd = ["mkfs.ntfs", "-Q"]
    
    mkfs_cmd.append(partition_path)
    success, out = run_command(mkfs_cmd)
    logs.append(out)
    
    return success, "".join(logs)
