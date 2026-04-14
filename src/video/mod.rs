pub const WIDTH: u32 = 640;
pub const HEIGHT: u32 = 400;
pub const PLANE_SIZE: usize = (WIDTH as usize * HEIGHT as usize) / 8; // 32000 bytes

pub struct VideoController {
    // 4 planes (Blue, Red, Green, Intensity)
    pub planes: [Vec<u8>; 4],
    pub palette: [[u8; 4]; 16],
    pub palette_index: u8,
    pub dirty: bool,
}

impl VideoController {
    pub fn new() -> Self {
        let mut palette = [[0u8; 4]; 16];
        // Default 16-color PC-98 palette (standard digital RGB)
        for i in 0..16 {
            palette[i] = [
                if (i & 2) != 0 { 255 } else { 0 }, // Red
                if (i & 4) != 0 { 255 } else { 0 }, // Green
                if (i & 1) != 0 { 255 } else { 0 }, // Blue
                255,                                // Alpha
            ];
            // Adjust intensity
            if (i & 8) != 0 {
                for c in 0..3 { if palette[i][c] == 0 { palette[i][c] = 85; } }
            } else {
                for c in 0..3 { if palette[i][c] == 255 { palette[i][c] = 170; } }
            }
        }

        Self {
            planes: [
                vec![0u8; PLANE_SIZE],
                vec![0u8; PLANE_SIZE],
                vec![0u8; PLANE_SIZE],
                vec![0u8; PLANE_SIZE],
            ],
            palette,
            palette_index: 0,
            dirty: true,
        }
    }

    pub fn render(&self, frame: &mut [u8]) {
        for y in 0..HEIGHT {
            for x_byte in 0..(WIDTH / 8) {
                let offset = (y * (WIDTH / 8) + x_byte) as usize;
                
                let b0 = self.planes[0][offset];
                let b1 = self.planes[1][offset];
                let b2 = self.planes[2][offset];
                let b3 = self.planes[3][offset];

                for bit in 0..8 {
                    let mask = 0x80 >> bit;
                    let mut color_idx = 0;
                    if (b0 & mask) != 0 { color_idx |= 1; }
                    if (b1 & mask) != 0 { color_idx |= 2; }
                    if (b2 & mask) != 0 { color_idx |= 4; }
                    if (b3 & mask) != 0 { color_idx |= 8; }

                    let color = self.palette[color_idx as usize];
                    let x = x_byte * 8 + bit;
                    let pixel_idx = ((y * WIDTH + x) * 4) as usize;
                    frame[pixel_idx..pixel_idx+4].copy_from_slice(&color);
                }
            }
        }
    }

    pub fn write_char(&self, ch: u8) {
        if ch == b'\n' || ch == b'\r' {
            println!();
        } else if ch.is_ascii_graphic() || ch == b' ' {
            print!("{}", ch as char);
        } else {
            print!("[{:02X}]", ch);
        }
        use std::io::Write;
        std::io::stdout().flush().unwrap();
    }

    pub fn handle_int10(&self, ah: u8, al: u8, bx: u16, cx: u16, dx: u16) {
        log::debug!("Video INT 10h AH={:02X} AL={:02X} BX={:04X} CX={:04X} DX={:04X}", ah, al, bx, cx, dx);
        if ah == 0x0E || ah == 0x09 || ah == 0x0A {
            self.write_char(al);
        }
    }

    pub fn write_port(&mut self, port: u16, val: u8) {
        match port {
            0xA8 => self.palette_index = val & 0x0F,
            0xAA => {
                // Blue (4 bits)
                self.palette[self.palette_index as usize][2] = (val & 0x0F) << 4 | (val & 0x0F);
                self.dirty = true;
            }
            0xAC => {
                // Red (4 bits)
                self.palette[self.palette_index as usize][0] = (val & 0x0F) << 4 | (val & 0x0F);
                self.dirty = true;
            }
            0xAE => {
                // Green (4 bits)
                self.palette[self.palette_index as usize][1] = (val & 0x0F) << 4 | (val & 0x0F);
                self.dirty = true;
            }
            _ => {}
        }
    }

    pub fn read_port(&self, port: u16) -> u8 {
        match port {
            0x00 => {
                // VSync status (PC-98)
                // Bit 5 is V-Blank. Use a simple alternating value or just 0 for now.
                // However, many games wait for it to be 1 then 0.
                if std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() % 16 < 8 {
                    0x20 // V-Blank on
                } else {
                    0x00 // V-Blank off
                }
            }
            _ => 0xFF,
        }
    }

    /// Update VRAM from physical memory write.
    /// Range A8000-BFFFF (Plane 0-2) and E0000-E7FFF (Plane 3)
    pub fn update_vram(&mut self, addr: u32, val: u8) -> bool {
        let (plane, offset) = match addr {
            0xA8000..=0xAFFFF => (0, addr - 0xA8000),
            0xB0000..=0xB7FFF => (1, addr - 0xB0000),
            0xB8000..=0xBFFFF => (2, addr - 0xB8000),
            0xE0000..=0xE7FFF => (3, addr - 0xE0000),
            _ => return false,
        };

        if (offset as usize) < PLANE_SIZE {
            self.planes[plane][offset as usize] = val;
            self.dirty = true;
            return true;
        }
        false
    }
}
