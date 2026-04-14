use crate::runtime::Runtime;
use crate::cpu::{Reg8, Reg16, SegReg};
use crate::memory::Memory;

impl Runtime {
    pub fn handle_int13(&mut self) {
        let ah = self.cpu.regs.get8(Reg8::AH);
        let al = self.cpu.regs.get8(Reg8::AL);
        let ch = self.cpu.regs.get8(Reg8::CH);
        let cl = self.cpu.regs.get8(Reg8::CL);
        let dh = self.cpu.regs.get8(Reg8::DH);
        let dl = self.cpu.regs.get8(Reg8::DL);
        let es = self.cpu.regs.get_seg(SegReg::ES);
        let bx = self.cpu.regs.get16(Reg16::BX);

        log::debug!("INT 13h AH={:02X} AL={} CH={} CL={} DH={} DL={} ES:BX={:04X}:{:04X}",
            ah, al, ch, cl, dh, dl, es, bx);

        match ah {
            0x00 => {
                // Reset disk
                self.cpu.regs.set8(Reg8::AH, 0);
                self.cpu.regs.set_cf(false);
            }
            0x02 => {
                // Read sectors
                let cylinder = (ch as u16) | (((cl as u16) & 0xC0) << 2);
                let sector = cl & 0x3F;
                let head = dh;
                let count = al as u32;

                if let Some(disk) = &self.bus.disk {
                    let lba = disk.chs_to_lba(cylinder, head, sector);
                    match disk.read_sectors(lba, count) {
                        Ok(data) => {
                            let dest = Memory::phys(es, bx as u32);
                            self.bus.mem.load_bytes(dest, &data);
                            self.cpu.regs.set8(Reg8::AH, 0x00);
                            self.cpu.regs.set8(Reg8::AL, count as u8);
                            self.cpu.regs.set_cf(false);
                        }
                        Err(e) => {
                            log::warn!("INT 13h/02: Read failed: {}", e);
                            self.cpu.regs.set8(Reg8::AH, 0x04);
                            self.cpu.regs.set_cf(true);
                        }
                    }
                } else {
                    self.cpu.regs.set8(Reg8::AH, 0x01);
                    self.cpu.regs.set_cf(true);
                }
            }
            0x03 | 0x04 => {
                // Write/Verify - success stub
                self.cpu.regs.set8(Reg8::AH, 0x00);
                self.cpu.regs.set_cf(false);
            }
            0x08 => {
                // Get parameters
                if let Some(disk) = &self.bus.disk {
                    let cyls = disk.cylinders.saturating_sub(2);
                    self.cpu.regs.set8(Reg8::CH, (cyls & 0xFF) as u8);
                    self.cpu.regs.set8(Reg8::CL, (disk.sectors_per_track & 0x3F) | (((cyls >> 8) & 3) as u8) << 6);
                    self.cpu.regs.set8(Reg8::DH, disk.heads.saturating_sub(1));
                    self.cpu.regs.set8(Reg8::DL, 1);
                    self.cpu.regs.set8(Reg8::AH, 0x00);
                    self.cpu.regs.set_cf(false);
                } else {
                    self.cpu.regs.set_cf(true);
                }
            }
            0x15 => {
                // Get disk type
                self.cpu.regs.set8(Reg8::AH, 0x03);
                self.cpu.regs.set_cf(false);
            }
            _ => {
                self.cpu.regs.set8(Reg8::AH, 0x01);
                self.cpu.regs.set_cf(true);
            }
        }
    }
}
