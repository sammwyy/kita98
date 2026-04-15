use crate::runtime::Runtime;
use crate::cpu::{Reg8, Reg16, SegReg};
use crate::memory::Memory;

impl Runtime {
    pub fn handle_int21(&mut self) {
        let ah = self.cpu.regs.get8(Reg8::AH);
        let al = self.cpu.regs.get8(Reg8::AL);
        
        let func_name = match ah {
            0x02 => "DisplayChar",
            0x09 => "PrintString",
            0x19 => "GetDefaultDrive",
            0x30 => "GetVersion",
            0x3D => "OpenFile",
            0x3F => "ReadFile",
            0x42 => "SeekFile",
            0x47 => "GetCurrentDir",
            0x4B => "ExecuteProgram",
            0x4C => "Exit",
            _ => "Unknown",
        };

        // Throttle spammy/unknown logs to avoid lagging the user's terminal
        if func_name != "Unknown" || self.cpu.instructions_executed % 1000 == 0 {
            log::info!("INT 21h AH={:02X} AL={:02X} ({})", ah, al, func_name);
        }

        match ah {
            0x02 => {
                // Display character
                let dl = self.cpu.regs.get8(Reg8::DL);
                self.bus.video.write_char(dl);
            }
            0x09 => {
                // Display string (terminated by $)
                let ds = self.cpu.regs.get_seg(SegReg::DS);
                let mut off = self.cpu.regs.get16(Reg16::DX);
                loop {
                    let c = self.bus.mem.seg_read_u8(ds, off as u32);
                    if c == b'$' {
                        break;
                    }
                    self.bus.video.write_char(c);
                    off = off.wrapping_add(1);
                }
            }
            0x19 => {
                // Get current default drive (0=A, 1=B, 2=C)
                self.cpu.regs.set8(Reg8::AL, 2); // Default to C:
            }
            0x1A => {
                // Set DTA (Disk Transfer Area)
                // DS:DX point to DTA
                let ds = self.cpu.regs.get_seg(SegReg::DS);
                let dx = self.cpu.regs.get16(Reg16::DX);
                log::info!("  -> DTA set to {:04X}:{:04X}", ds, dx);
                // In a more complete DOS emu we'd store this, but for now we just log
            }
            0x25 => {
                // Set Interrupt Vector
                // AL = interrupt number
                // DS:DX = pointer to handler
                let ds = self.cpu.regs.get_seg(SegReg::DS);
                let dx = self.cpu.regs.get16(Reg16::DX);
                log::info!("  -> Setting INT {:02X}h to {:04X}:{:04X}", al, ds, dx);
                self.bus.mem.set_ivt(al, ds, dx);
            }
            0x2C => {
                // Get System Time
                // CH:CL = HH:MM, DH:DL = SS:CC
                self.cpu.regs.set16(Reg16::CX, 0x1200);
                self.cpu.regs.set16(Reg16::DX, 0x0000);
            }
            0x30 => {
                // Get DOS version (return 5.0)
                self.cpu.regs.set8(Reg8::AL, 5);    // Major
                self.cpu.regs.set8(Reg8::AH, 0);    // Minor
                self.cpu.regs.set8(Reg8::BH, 0xFF); // MS-DOS
                self.cpu.regs.set16(Reg16::CX, 0x0000);
            }
            0x33 => {
                // Get/Set Ctrl-Break flag
                self.cpu.regs.set8(Reg8::DL, 0); 
                self.cpu.regs.set_cf(false);
            }
            0x34 => {
                // Get address of InDOS flag (stub)
                self.cpu.regs.set_seg(SegReg::ES, 0x0000);
                self.cpu.regs.set16(Reg16::BX, 0x0000);
            }
            0x35 => {
                // Get Interrupt Vector
                let (seg, off) = self.bus.mem.get_ivt(al);
                self.cpu.regs.set_seg(SegReg::ES, seg);
                self.cpu.regs.set16(Reg16::BX, off);
            }
            0x44 => {
                // IOCTL
                // Return success for common devices
                self.cpu.regs.set_cf(false);
            }
            0x48 => {
                // Allocate Memory
                // BX = number of paragraphs
                // Return AX = segment of allocated block
                // (HACK: Just return a high but safe segment like 0x8000)
                let bx = self.cpu.regs.get16(Reg16::BX);
                log::info!("  -> Allocating {} paragraphs", bx);
                self.cpu.regs.set16(Reg16::AX, 0x8000);
                self.cpu.regs.set_cf(false);
            }
            0x49 => {
                // Free Memory
                // ES = segment of block
                self.cpu.regs.set_cf(false);
            }
            0x4A => {
                // Resize Memory Block
                // ES = segment, BX = new size
                self.cpu.regs.set_cf(false);
            }
            0x3D => {
                // Open file
                let ds_seg = self.cpu.regs.get_seg(SegReg::DS);
                let dx_off = self.cpu.regs.get16(Reg16::DX);
                let mut path = String::new();
                let mut off = dx_off;
                loop {
                    let c = self.bus.mem.seg_read_u8(ds_seg, off as u32);
                    if c == 0 { break; }
                    path.push(c as char);
                    off = off.wrapping_add(1);
                }
                path = path.to_uppercase();
                
                // Try to find on disk image first
                let mut found_on_disk = false;
                if let Some(disk) = &self.bus.disk {
                    if let Ok(fs) = crate::disk::fat::FatFileSystem::detect(disk) {
                        if let Ok(entries) = fs.list_root_dir(disk) {
                            if let Some(entry) = entries.iter().find(|e| e.full_name().to_uppercase() == path) {
                                if let Ok(data) = fs.read_file(disk, entry) {
                                    let handle = 100 + self.dos.files.len() as u16;
                                    self.dos.files.insert(handle, crate::dos::FileHandle::Virtual(std::io::Cursor::new(data)));
                                    self.cpu.regs.set16(Reg16::AX, handle);
                                    self.cpu.regs.set_cf(false);
                                    found_on_disk = true;
                                    log::info!("  -> Served from disk image (handle {})", handle);
                                }
                            }
                        }
                    }
                }

                if !found_on_disk {
                    match self.dos.open(&path) {
                        Ok(handle) => {
                            self.cpu.regs.set16(Reg16::AX, handle);
                            self.cpu.regs.set_cf(false);
                        }
                        Err(_) => {
                            self.cpu.regs.set16(Reg16::AX, 2); // File not found
                            self.cpu.regs.set_cf(true);
                        }
                    }
                }
            }
            0x3F => {
                // Read from file
                let handle = self.cpu.regs.get16(Reg16::BX);
                let count = self.cpu.regs.get16(Reg16::CX) as usize;
                let ds_seg = self.cpu.regs.get_seg(SegReg::DS);
                let dx_off = self.cpu.regs.get16(Reg16::DX);
                let mut buf = vec![0u8; count];
                match self.dos.read(handle, &mut buf) {
                    Ok(n) => {
                        self.bus.mem.load_bytes(Memory::phys(ds_seg, dx_off as u32), &buf[..n]);
                        self.cpu.regs.set16(Reg16::AX, n as u16);
                        self.cpu.regs.set_cf(false);
                    }
                    Err(_) => {
                        self.cpu.regs.set_cf(true);
                    }
                }
            }
            0x42 => {
                // Seek
                let handle = self.cpu.regs.get16(Reg16::BX);
                let whence = al;
                let offset_high = self.cpu.regs.get16(Reg16::CX) as i64;
                let offset_low = self.cpu.regs.get16(Reg16::DX) as i64;
                let offset = (offset_high << 16) | offset_low;
                match self.dos.seek(handle, offset, whence) {
                    Ok(pos) => {
                        self.cpu.regs.set16(Reg16::AX, (pos & 0xFFFF) as u16);
                        self.cpu.regs.set16(Reg16::DX, (pos >> 16) as u16);
                        self.cpu.regs.set_cf(false);
                    }
                    Err(_) => {
                        self.cpu.regs.set_cf(true);
                    }
                }
            }
            0x47 => {
                // Get current directory
                let ds = self.cpu.regs.get_seg(SegReg::DS);
                let si = self.cpu.regs.get16(Reg16::SI);
                self.bus.mem.seg_write_u8(ds, si as u32, 0); // Null terminator
                self.cpu.regs.set8(Reg8::AL, 0);
                self.cpu.regs.set_cf(false);
            }
            0x4B => {
                // Execute Program
                if al == 0x00 { // Load and execute
                    let ds_seg = self.cpu.regs.get_seg(SegReg::DS);
                    let dx_off = self.cpu.regs.get16(Reg16::DX);
                    let mut path = String::new();
                    let mut off = dx_off;
                    loop {
                        let c = self.bus.mem.seg_read_u8(ds_seg, off as u32);
                        if c == 0 { break; }
                        path.push(c as char);
                        off = off.wrapping_add(1);
                    }
                    path = path.to_uppercase();
                    log::info!("DOS EXEC: {}", path);

                    let mut loaded = false;
                    if let Some(disk) = &self.bus.disk {
                        if let Ok(fs) = crate::disk::fat::FatFileSystem::detect(disk) {
                            if let Ok(entries) = fs.list_root_dir(disk) {
                                if let Some(entry) = entries.iter().find(|e| e.full_name().to_uppercase() == path) {
                                    if let Ok(data) = fs.read_file(disk, entry) {
                                        log::info!("  -> Chain-loading {} ({} bytes)", path, data.len());
                                        self.cpu.regs = crate::cpu::regs::Regs::new();
                                        if let Err(e) = Self::execute_binary(&mut self.bus, &mut self.cpu, &entry.name, &entry.ext, &data) {
                                            log::error!("Failed to execute chain-loaded binary: {}", e);
                                        } else {
                                            loaded = true;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if loaded {
                        self.cpu.regs.set_cf(false);
                    } else {
                        self.cpu.regs.set16(Reg16::AX, 2);
                        self.cpu.regs.set_cf(true);
                    }
                } else {
                    self.cpu.regs.set_cf(true);
                }
            }
            0x4C => {
                // Exit with return code
                log::info!("INT 21h/4C – program exit (code={})", al);
                self.cpu.halted = true;
            }
            _ => {
                self.cpu.regs.set_cf(true);
            }
        }
    }
}
