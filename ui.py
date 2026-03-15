import gi
gi.require_version('Gtk', '3.0')
from gi.repository import Gtk, GLib
import threading
import backend

class LiveRaidWindow(Gtk.Window):
    def __init__(self):
        super().__init__(title="LiveRAID Configurator")
        self.set_border_width(15)
        self.set_default_size(640, 600)  # Increased default window size

        # Main Vertical Box
        vbox = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=15)
        self.add(vbox)

        # --- SECTION 1: ARRAY CREATION ---
        frame_create = Gtk.Frame(label=" 1. Create Array ")
        frame_create.set_shadow_type(Gtk.ShadowType.ETCHED_IN)
        vbox.pack_start(frame_create, False, False, 5)
        
        vbox_create = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        vbox_create.set_border_width(10)
        frame_create.add(vbox_create)

        lbl_drives = Gtk.Label(label="Select Target Drives:", xalign=0)
        vbox_create.pack_start(lbl_drives, False, False, 0)
        
        self.drive_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        vbox_create.pack_start(self.drive_box, False, False, 0)
        
        self.drive_checkboxes = {}
        self.refresh_drives()

        hbox_raid = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=10)
        lbl_raid = Gtk.Label(label="RAID Level:", xalign=0)
        hbox_raid.pack_start(lbl_raid, False, False, 0)
        
        self.combo_raid = Gtk.ComboBoxText()
        for level in ["0", "1", "5", "10"]:
            self.combo_raid.append_text(level)
        self.combo_raid.set_active(1)
        hbox_raid.pack_start(self.combo_raid, True, True, 0)
        
        lbl_chunk = Gtk.Label(label="Chunk Size:", xalign=0)
        hbox_raid.pack_start(lbl_chunk, False, False, 0)
        self.combo_chunk = Gtk.ComboBoxText()
        for chunk in ["Default", "64K", "128K", "256K", "512K", "1024K"]:
            self.combo_chunk.append_text(chunk)
        self.combo_chunk.set_active(0)
        hbox_raid.pack_start(self.combo_chunk, False, False, 0)
        vbox_create.pack_start(hbox_raid, False, False, 0)

        self.chk_ssd = Gtk.CheckButton(label="Assume SSD (Skip initial sync)")
        vbox_create.pack_start(self.chk_ssd, False, False, 0)

        self.btn_create = Gtk.Button(label="Create RAID Array")
        self.btn_create.connect("clicked", self.on_create_clicked)
        self.btn_create.get_style_context().add_class("suggested-action")
        vbox_create.pack_start(self.btn_create, False, False, 5)

        # --- SECTION 2: ARRAY MANAGEMENT & FORMATTING ---
        frame_manage = Gtk.Frame(label=" 2. Manage & Format Arrays ")
        frame_manage.set_shadow_type(Gtk.ShadowType.ETCHED_IN)
        vbox.pack_start(frame_manage, False, False, 5)
        
        vbox_manage = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        vbox_manage.set_border_width(10)
        frame_manage.add(vbox_manage)

        hbox_arrays = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=10)
        lbl_arrays = Gtk.Label(label="Detected Active Arrays:", xalign=0)
        hbox_arrays.pack_start(lbl_arrays, False, False, 0)
        
        self.combo_arrays = Gtk.ComboBoxText()
        hbox_arrays.pack_start(self.combo_arrays, True, True, 0)
        
        self.btn_refresh = Gtk.Button(label="Refresh")
        self.btn_refresh.connect("clicked", self.refresh_arrays)
        hbox_arrays.pack_start(self.btn_refresh, False, False, 0)
        
        self.btn_delete = Gtk.Button(label="Stop & Delete Array")
        self.btn_delete.connect("clicked", self.on_delete_clicked)
        self.btn_delete.get_style_context().add_class("destructive-action")
        hbox_arrays.pack_start(self.btn_delete, False, False, 0)
        vbox_manage.pack_start(hbox_arrays, False, False, 0)

        # Formatting Options
        hbox_fs = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=10)
        lbl_fs = Gtk.Label(label="Filesystem:", xalign=0)
        hbox_fs.pack_start(lbl_fs, False, False, 0)
        
        self.combo_fs = Gtk.ComboBoxText()
        for fs in ["ext4", "btrfs", "xfs", "zfs", "f2fs", "exfat", "ntfs", "vfat"]:
            self.combo_fs.append_text(fs)
        self.combo_fs.set_active(0)
        hbox_fs.pack_start(self.combo_fs, True, True, 0)
        vbox_manage.pack_start(hbox_fs, False, False, 0)

        grid_opts = Gtk.Grid(column_spacing=15, row_spacing=5)
        vbox_manage.pack_start(grid_opts, False, False, 0)

        self.chk_boot = Gtk.CheckButton(label="Set partition as Bootable")
        grid_opts.attach(self.chk_boot, 0, 0, 1, 1)
        
        self.chk_trim = Gtk.CheckButton(label="Enable TRIM/Discard support")
        grid_opts.attach(self.chk_trim, 0, 1, 1, 1)

        self.btn_format = Gtk.Button(label="Format Selected Array")
        self.btn_format.connect("clicked", self.on_format_clicked)
        self.btn_format.get_style_context().add_class("suggested-action")
        vbox_manage.pack_start(self.btn_format, False, False, 5)

        self.refresh_arrays()

        # --- SECTION 3: EXECUTION LOG ---
        lbl_logs = Gtk.Label(label="<b>Execution Log:</b>", use_markup=True, xalign=0)
        vbox.pack_start(lbl_logs, False, False, 0)
        
        scrolled_window = Gtk.ScrolledWindow()
        scrolled_window.set_hexpand(True)
        scrolled_window.set_vexpand(True)
        scrolled_window.set_min_content_height(300) # Ensure the log viewer is at least double standard height
        vbox.pack_start(scrolled_window, True, True, 0)

        self.text_view = Gtk.TextView()
        self.text_view.set_editable(False)
        self.text_view.set_wrap_mode(Gtk.WrapMode.WORD)
        self.text_buffer = self.text_view.get_buffer()
        
        # Style the text view slightly to look more like a log
        self.text_view.modify_font(gi.repository.Pango.FontDescription('Monospace 10'))
        
        scrolled_window.add(self.text_view)
        
        if backend.DRY_RUN:
            self.append_log("--- DRY RUN MODE IS ACTIVE ---\n")
            self.append_log("System commands will be logged but NOT executed against disks.\n\n")

    def append_log(self, text):
        end_iter = self.text_buffer.get_end_iter()
        self.text_buffer.insert(end_iter, text)
        
        # Auto-scroll to the bottom of the log
        mark = self.text_buffer.create_mark(None, self.text_buffer.get_end_iter(), False)
        self.text_view.scroll_to_mark(mark, 0.0, True, 0.0, 1.0)

    def refresh_drives(self):
        # Clear existing checkboxes
        for child in self.drive_box.get_children():
            self.drive_box.remove(child)
            
        self.drive_checkboxes = {}
        drives = backend.get_available_drives()
        
        if not drives:
            lbl_no_drives = Gtk.Label(label="No available unmounted physical drives detected.", xalign=0)
            self.drive_box.pack_start(lbl_no_drives, False, False, 0)
        else:
            for d in drives:
                cb = Gtk.CheckButton(label=f"{d['name']} ({d['size_gb']} GB)")
                self.drive_checkboxes[d['name']] = cb
                self.drive_box.pack_start(cb, False, False, 0)
        
        self.drive_box.show_all()

    def refresh_arrays(self, widget=None):
        self.combo_arrays.remove_all()
        active_arrays = backend.get_active_arrays()
        self.refresh_drives() # Always refresh the available physical disks too
        
        if not active_arrays:
            self.combo_arrays.append_text("No active arrays found")
            self.combo_arrays.set_sensitive(False)
            self.btn_delete.set_sensitive(False)
            self.btn_format.set_sensitive(False)
        else:
            for arr in active_arrays:
                self.combo_arrays.append_text(f"{arr['name']} ({arr['type']} - {arr['status']})")
            self.combo_arrays.set_sensitive(True)
            self.btn_delete.set_sensitive(True)
            self.btn_format.set_sensitive(True)
        self.combo_arrays.set_active(0)

    def on_create_clicked(self, widget):
        selected_drives = [name for name, cb in self.drive_checkboxes.items() if cb.get_active()]
        
        if len(selected_drives) == 0:
            self.append_log("ERROR: No drives selected for creation.\n")
            return
            
        raid_level = self.combo_raid.get_active_text()
        chunk_size = self.combo_chunk.get_active_text()
        ssd_mode = self.chk_ssd.get_active()
        
        self.btn_create.set_sensitive(False)
        self.append_log(f"\n--- Creating Array ---\nTasks: RAID {raid_level} -> {len(selected_drives)} devices\n")
        
        thread = threading.Thread(
            target=self.execute_create,
            args=(selected_drives, raid_level, chunk_size, ssd_mode)
        )
        thread.daemon = True
        thread.start()

    def execute_create(self, drives, raid_level, chunk_size, ssd_mode):
        def update_ui(msg, finish=False):
            GLib.idle_add(self.append_log, msg)
            if finish:
                GLib.idle_add(self.btn_create.set_sensitive, True)
                GLib.idle_add(self.refresh_arrays)
                
        # Typically the first array defaults to /dev/md0
        array_name = "/dev/md0"
        
        update_ui("-> Generating Array via mdadm...\n")
        success, out = backend.create_raid(raid_level, drives, array_name, chunk_size, ssd_mode)
        update_ui(out)
        
        if success:
            update_ui("\nSUCCESS: Array creation dispatched.\n", True)
        else:
            update_ui("\nERROR: RAID creation failed.\n", True)

    def on_delete_clicked(self, widget):
        arr_text = self.combo_arrays.get_active_text()
        if not arr_text or "No active arrays" in arr_text:
            return
            
        array_name = arr_text.split(" ")[0] # Extract '/dev/md0'
        
        dialog = Gtk.MessageDialog(
            transient_for=self,
            flags=0,
            message_type=Gtk.MessageType.WARNING,
            buttons=Gtk.ButtonsType.OK_CANCEL,
            text=f"Delete {array_name}?"
        )
        dialog.format_secondary_text("This will stop the RAID array and zero the superblocks, effectively destroying the array geometry and any data spanning across the physical drives.")
        response = dialog.run()
        dialog.destroy()
        
        if response == Gtk.ResponseType.OK:
            self.btn_delete.set_sensitive(False)
            self.append_log(f"\n--- Destroying Array {array_name} ---\n")
            thread = threading.Thread(target=self.execute_delete, args=(array_name,))
            thread.daemon = True
            thread.start()

    def execute_delete(self, array_name):
        def update_ui(msg, finish=False):
            GLib.idle_add(self.append_log, msg)
            if finish:
                GLib.idle_add(self.refresh_arrays)
                
        success, out = backend.delete_raid(array_name)
        update_ui(out)
        if success:
            update_ui(f"SUCCESS: {array_name} stopped and metadata cleared.\n", True)
        else:
            update_ui(f"ERROR: Failed to cleanly destroy {array_name}.\n", True)

    def on_format_clicked(self, widget):
        arr_text = self.combo_arrays.get_active_text()
        if not arr_text or "No active arrays" in arr_text:
            return
            
        array_name = arr_text.split(" ")[0] # Extract '/dev/md0'
        fs_type = self.combo_fs.get_active_text()
        boot_flag = self.chk_boot.get_active()
        trim_discard = self.chk_trim.get_active()
        
        self.btn_format.set_sensitive(False)
        self.append_log(f"\n--- Formatting Array {array_name} ---\n")
        self.append_log(f"Tasks: Create GPT -> Primary Partition -> mkfs.{fs_type}\n")
        
        thread = threading.Thread(
            target=self.execute_format,
            args=(array_name, fs_type, boot_flag, trim_discard)
        )
        thread.daemon = True
        thread.start()

    def execute_format(self, array_name, fs_type, boot_flag, trim_discard):
        def update_ui(msg, finish=False):
            GLib.idle_add(self.append_log, msg)
            if finish:
                GLib.idle_add(self.btn_format.set_sensitive, True)
                
        update_ui("-> Partitioning & Formatting via parted/mkfs...\n")
        success, out = backend.format_device(array_name, fs_type, boot_flag, trim_discard)
        update_ui(out)
        
        if success:
            update_ui("\nSUCCESS: Partitioning and formatting completed.\n", True)
        else:
            update_ui("\nERROR: Failed during filesystem generation.\n", True)
