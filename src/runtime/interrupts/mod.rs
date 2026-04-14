pub mod video;
pub mod disk;
pub mod dos;
pub mod system;

use crate::runtime::Runtime;

impl Runtime {
    /// Returns true if the interrupt was handled natively (no IVT jump needed).
    pub fn handle_interrupt(&mut self, num: u8) -> bool {
        match num {
            0x10 => { self.handle_int10(); true }
            0x11 => { self.handle_int11(); true }
            0x12 => { self.handle_int12(); true }
            0x13 => { self.handle_int13(); true }
            0x15 => { self.handle_int15(); true }
            0x16 => { self.handle_int16(); true }
            0x18 => { self.handle_int18(); true } // PC-98 Video BIOS
            0x19 => { // Bootstrap (Warm Reboot)
                log::info!("INT 19h – warm reboot");
                self.cpu.halted = true;
                true
            }
            0x1A => { self.handle_int1a(); true }
            0x1F => { self.handle_int1f(); true } // PC-98 Sound/System BIOS
            0x20 => {
                log::info!("INT 20h – program terminate");
                self.cpu.halted = true;
                true
            }
            0x21 => { self.handle_int21(); true }
            _ => {
                log::debug!("INT {:02X}h – using IVT fallback", num);
                false
            }
        }
    }
}
