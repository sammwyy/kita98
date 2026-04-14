use crate::runtime::Runtime;
use crate::cpu::{Reg8, Reg16, SegReg};
use crate::memory::Memory;

impl Runtime {
    pub fn handle_int21(&mut self) {
        let ah = self.cpu.regs.get8(Reg8::AH);
        let al = self.cpu.regs.get8(Reg8::AL);
        log::debug!("INT 21h AH={:02X} AL={:02X}", ah, al);

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
                match self.dos.open(&path) {
                    Ok(handle) => {
                        self.cpu.regs.set16(Reg16::AX, handle);
                        self.cpu.regs.set_cf(false);
                    }
                    Err(_) => {
                        self.cpu.regs.set16(Reg16::AX, 0x02); // File not found
                        self.cpu.regs.set_cf(true);
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
            0x4C => {
                // Exit with return code
                log::info!("INT 21h/4C – program exit (code={})", al);
                self.cpu.halted = true;
            }
            _ => {
                self.cpu.regs.set_cf(true); // Unsupported sub-function
            }
        }
    }
}
