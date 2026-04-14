use crate::runtime::Runtime;
use crate::cpu::{Reg8, Reg16};

impl Runtime {
    pub fn handle_int10(&mut self) {
        let ah = self.cpu.regs.get8(Reg8::AH);
        let al = self.cpu.regs.get8(Reg8::AL);
        let bx = self.cpu.regs.get16(Reg16::BX);
        let cx = self.cpu.regs.get16(Reg16::CX);
        let dx = self.cpu.regs.get16(Reg16::DX);
        
        log::info!("INT 10h AH={:02X} AL={:02X}", ah, al);
        self.bus.video.handle_int10(ah, al, bx, cx, dx);

        match ah {
            0x03 => {
                // Get cursor position – return (0, 0)
                self.cpu.regs.set16(Reg16::DX, 0x0000);
            }
            0x0F => {
                // Get video mode
                self.cpu.regs.set8(Reg8::AL, 0x03); // 80x25 text
                self.cpu.regs.set8(Reg8::AH, 80);   // 80 columns
            }
            _ => {}
        }
    }

    pub fn handle_int18(&mut self) {
        let ah = self.cpu.regs.get8(Reg8::AH);
        log::info!("INT 18h AH={:02X} (PC-98 Video)", ah);
        
        match ah {
            0x0a => {
                // Get GDC status
                // Return dummy status (ready)
                self.cpu.regs.set8(Reg8::AL, 0x00);
            }
            0x40 => {
                // Screen initialization / Display ON
                self.bus.video.dirty = true;
            }
            0x41 => {
                // Set screen mode
                // AL = mode bits
            }
            0x42 => {
                // GDC control
                // CH = 0 (GDC1), 1 (GDC2)
                // CL = command
            }
            _ => {
                log::warn!("Unhandled INT 18h AH={:02X}", ah);
            }
        }
    }
}
