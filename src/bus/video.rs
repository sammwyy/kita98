pub struct VideoDevice {
    pub vram: Vec<u8>,
}

impl VideoDevice {
    pub fn new() -> Self {
        Self {
            vram: vec![0u8; 640 * 400 * 4], // Placeholder for 640x400 RGBA
        }
    }

    pub fn write_char(&mut self, c: u8) {
        // Minimal text-mode emulation: just print to stdout for now.
        print!("{}", c as char);
        use std::io::Write;
        std::io::stdout().flush().unwrap();
    }

    pub fn handle_int10(&mut self, ah: u8, al: u8, bx: u16, cx: u16, dx: u16) {
        log::trace!("INT 10h stub: AH={:02X} AL={:02X} BX={:04X} CX={:04X} DX={:04X}", ah, al, bx, cx, dx);
        match ah {
            0x0E => {
                // Teletype output
                self.write_char(al);
            }
            _ => {
                // Many PC-98 games use different I/O or INT 18h for video.
                // INT 10h is more of a DOS-ism/IBM-ism.
            }
        }
    }

    pub fn io_write(&mut self, port: u16, val: u8) {
        log::trace!("Video I/O Write: Port={:04X} Val={:02X}", port, val);
    }

    pub fn io_read(&mut self, port: u16) -> u8 {
        log::trace!("Video I/O Read: Port={:04X}", port);
        0
    }
}
