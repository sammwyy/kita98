use crate::runtime::Runtime;
use crate::cpu::{Reg8, Reg16};

impl Runtime {
    pub fn handle_int11(&mut self) {
        // Equipment Check: Return floppy + VGA
        self.cpu.regs.set16(Reg16::AX, 0x0021);
    }

    pub fn handle_int12(&mut self) {
        // Memory Size: 640 KB
        self.cpu.regs.set16(Reg16::AX, 640);
    }

    pub fn handle_int15(&mut self) {
        let ah = self.cpu.regs.get8(Reg8::AH);
        log::debug!("INT 15h AH={:02X} – unsupported stub", ah);
        self.cpu.regs.set_cf(true);
    }

    pub fn handle_int16(&mut self) {
        let ah = self.cpu.regs.get8(Reg8::AH);
        log::debug!("INT 16h AH={:02X}", ah);
        match ah {
            0x00 => {
                // Read key – return 0 (no input available)
                self.cpu.regs.set16(Reg16::AX, 0);
            }
            0x01 => {
                // Check key available – set ZF if no key
                self.cpu.regs.set_zf(true);
            }
            0x02 => {
                // Get shift flags
                self.cpu.regs.set8(Reg8::AL, 0);
            }
            _ => {
                self.cpu.regs.set_cf(true);
            }
        }
    }

    pub fn handle_int1a(&mut self) {
        let ah = self.cpu.regs.get8(Reg8::AH);
        match ah {
            0x00 => {
                // Get system time ticks
                let t = self.bus.timer.ticks;
                self.cpu.regs.set16(Reg16::CX, (t >> 16) as u16);
                self.cpu.regs.set16(Reg16::DX, (t & 0xFFFF) as u16);
                self.cpu.regs.set8(Reg8::AL, 0); // Midnight flag
            }
            _ => {
                self.cpu.regs.set_cf(true);
            }
        }
    }
}
