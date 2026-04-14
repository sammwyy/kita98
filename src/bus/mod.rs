use crate::audio::AudioController;
use crate::devices::{KeyboardDevice, TimerDevice};
use crate::disk::Disk;
use crate::memory::Memory;
use crate::video::VideoController;

pub struct Bus {
    pub mem: Memory,
    pub video: VideoController,
    pub audio: AudioController,
    pub keyboard: KeyboardDevice,
    pub timer: TimerDevice,
    pub disk: Option<Disk>,
}

#[allow(dead_code)]
impl Bus {
    pub fn new(disk: Option<Disk>) -> Self {
        Self {
            mem: Memory::new(),
            video: VideoController::new(),
            audio: AudioController::new(),
            keyboard: KeyboardDevice::new(),
            timer: TimerDevice::new(),
            disk,
        }
    }

    // Memory access

    pub fn mem_read_u8(&self, addr: u32) -> u8 {
        self.mem.read_u8(addr)
    }

    pub fn mem_write_u8(&mut self, addr: u32, val: u8) {
        // Intercept VRAM
        if self.video.update_vram(addr, val) {
            return;
        }
        self.mem.write_u8(addr, val);
    }

    pub fn mem_read_u16(&self, addr: u32) -> u16 {
        self.mem.read_u16(addr)
    }

    pub fn mem_write_u16(&mut self, addr: u32, val: u16) {
        self.mem_write_u8(addr, val as u8);
        self.mem_write_u8(addr.wrapping_add(1), (val >> 8) as u8);
    }

    // I/O port stubs

    pub fn io_read_u8(&mut self, port: u16) -> u8 {
        log::trace!("IO RD8 port={:04X}", port);
        match port {
            0x60 => 0, // keyboard data
            0x61 => 0, // port B
            0x64 => 0, // keyboard status
            _ => 0xFF,
        }
    }

    pub fn io_write_u8(&mut self, port: u16, val: u8) {
        match port {
            0xA0 | 0x20 => {} // PIC – ignore
            0x70..=0x7E => {
                // PC-98 Video / Palette control
                self.video.dirty = true;
            }
            0x40..=0x46 => {
                // PIT (Timer) - stub
            }
            _ => {
                log::trace!("IO WR8 port={:04X} val={:02X}", port, val);
            }
        }
    }
}
