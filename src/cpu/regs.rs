/// x86 real-mode CPU registers and flags (386 compatible).

// Flags

/// Individual flag bits within the FLAGS register (32-bit EFLAGS).
pub mod flags {
    #[allow(dead_code)] pub const CF: u32 = 1 << 0; // Carry
    #[allow(dead_code)] pub const PF: u32 = 1 << 2; // Parity
    #[allow(dead_code)] pub const AF: u32 = 1 << 4; // Auxiliary carry
    #[allow(dead_code)] pub const ZF: u32 = 1 << 6; // Zero
    #[allow(dead_code)] pub const SF: u32 = 1 << 7; // Sign
    #[allow(dead_code)] pub const TF: u32 = 1 << 8; // Trap
    #[allow(dead_code)] pub const IF: u32 = 1 << 9; // Interrupt enable
    #[allow(dead_code)] pub const DF: u32 = 1 << 10; // Direction
    #[allow(dead_code)] pub const OF: u32 = 1 << 11; // Overflow
    #[allow(dead_code)] pub const IOPL: u32 = 3 << 12; // I/O Privilege Level
    #[allow(dead_code)] pub const NT: u32 = 1 << 14; // Nested Task
    #[allow(dead_code)] pub const RF: u32 = 1 << 16; // Resume Flag
    #[allow(dead_code)] pub const VM: u32 = 1 << 17; // Virtual 8086 Mode
}

// Registers

/// Register index for the 8-bit half-registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Reg8 {
    AL = 0,
    CL = 1,
    DL = 2,
    BL = 3,
    AH = 4,
    CH = 5,
    DH = 6,
    BH = 7,
}

/// Register index for the 16-bit / 32-bit general-purpose registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Reg16 {
    AX = 0,
    CX = 1,
    DX = 2,
    BX = 3,
    SP = 4,
    BP = 5,
    SI = 6,
    DI = 7,
}

/// Segment register index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SegReg {
    ES = 0,
    CS = 1,
    SS = 2,
    DS = 3,
    FS = 4,
    GS = 5,
}

// CPU state

#[derive(Debug, Clone)]
pub struct Regs {
    /// General-purpose registers (EAX..EDI) – 32-bit.
    pub gp: [u32; 8],
    /// Segment registers (ES, CS, SS, DS, FS, GS).
    pub seg: [u16; 6],
    /// Instruction pointer (EIP).
    pub ip: u32,
    /// Flags register (EFLAGS).
    pub flags: u32,
}

impl Regs {
    pub fn new() -> Self {
        let mut r = Self {
            gp: [0u32; 8],
            seg: [0u16; 6],
            ip: 0,
            flags: 0x0002, // bit 1 is always set in real hardware
        };
        // Sensible real-mode defaults
        r.set_seg(SegReg::CS, 0x0000);
        r.set_seg(SegReg::DS, 0x0000);
        r.set_seg(SegReg::ES, 0x0000);
        r.set_seg(SegReg::SS, 0x0000);
        r.set_seg(SegReg::FS, 0x0000);
        r.set_seg(SegReg::GS, 0x0000);
        r.set16(Reg16::SP, 0xFFFE);
        r
    }

    // 32-bit GP access

    #[inline(always)]
    pub fn get32(&self, r: Reg16) -> u32 {
        self.gp[r as usize]
    }

    #[inline(always)]
    pub fn set32(&mut self, r: Reg16, v: u32) {
        self.gp[r as usize] = v;
    }

    // 16-bit GP access

    #[inline(always)]
    pub fn get16(&self, r: Reg16) -> u16 {
        self.gp[r as usize] as u16
    }

    #[inline(always)]
    pub fn set16(&mut self, r: Reg16, v: u16) {
        self.gp[r as usize] = (self.gp[r as usize] & 0xFFFF0000) | (v as u32);
    }

    // 8-bit GP access (AH/AL etc.)--

    #[inline(always)]
    pub fn get8(&self, r: Reg8) -> u8 {
        match r {
            Reg8::AL => (self.gp[0] & 0xFF) as u8,
            Reg8::CL => (self.gp[1] & 0xFF) as u8,
            Reg8::DL => (self.gp[2] & 0xFF) as u8,
            Reg8::BL => (self.gp[3] & 0xFF) as u8,
            Reg8::AH => ((self.gp[0] >> 8) & 0xFF) as u8,
            Reg8::CH => ((self.gp[1] >> 8) & 0xFF) as u8,
            Reg8::DH => ((self.gp[2] >> 8) & 0xFF) as u8,
            Reg8::BH => ((self.gp[3] >> 8) & 0xFF) as u8,
        }
    }

    #[inline(always)]
    pub fn set8(&mut self, r: Reg8, v: u8) {
        match r {
            Reg8::AL => self.gp[0] = (self.gp[0] & 0xFFFFFF00) | v as u32,
            Reg8::CL => self.gp[1] = (self.gp[1] & 0xFFFFFF00) | v as u32,
            Reg8::DL => self.gp[2] = (self.gp[2] & 0xFFFFFF00) | v as u32,
            Reg8::BL => self.gp[3] = (self.gp[3] & 0xFFFFFF00) | v as u32,
            Reg8::AH => self.gp[0] = (self.gp[0] & 0xFFFF00FF) | ((v as u32) << 8),
            Reg8::CH => self.gp[1] = (self.gp[1] & 0xFFFF00FF) | ((v as u32) << 8),
            Reg8::DH => self.gp[2] = (self.gp[2] & 0xFFFF00FF) | ((v as u32) << 8),
            Reg8::BH => self.gp[3] = (self.gp[3] & 0xFFFF00FF) | ((v as u32) << 8),
        }
    }

    // Segment register access --

    #[inline(always)]
    pub fn get_seg(&self, s: SegReg) -> u16 {
        self.seg[s as usize]
    }

    #[inline(always)]
    pub fn set_seg(&mut self, s: SegReg, v: u16) {
        self.seg[s as usize] = v;
    }

    // Flag helpers--

    #[inline(always)]
    pub fn flag(&self, f: u32) -> bool {
        self.flags & f != 0
    }

    #[inline(always)]
    pub fn set_flag(&mut self, f: u32, v: bool) {
        if v {
            self.flags |= f;
        } else {
            self.flags &= !f;
        }
    }

    pub fn get_cf(&self) -> bool {
        self.flag(flags::CF)
    }
    pub fn get_zf(&self) -> bool {
        self.flag(flags::ZF)
    }
    pub fn get_sf(&self) -> bool {
        self.flag(flags::SF)
    }
    pub fn get_of(&self) -> bool {
        self.flag(flags::OF)
    }
    pub fn get_df(&self) -> bool {
        self.flag(flags::DF)
    }
    pub fn get_af(&self) -> bool {
        self.flag(flags::AF)
    }
    pub fn get_pf(&self) -> bool {
        self.flag(flags::PF)
    }

    pub fn set_cf(&mut self, v: bool) {
        self.set_flag(flags::CF, v)
    }
    pub fn set_zf(&mut self, v: bool) {
        self.set_flag(flags::ZF, v)
    }
    pub fn set_sf(&mut self, v: bool) {
        self.set_flag(flags::SF, v)
    }
    pub fn set_of(&mut self, v: bool) {
        self.set_flag(flags::OF, v)
    }
    pub fn set_df(&mut self, v: bool) {
        self.set_flag(flags::DF, v)
    }
    pub fn set_pf(&mut self, v: bool) {
        self.set_flag(flags::PF, v)
    }
    #[allow(dead_code)]
    pub fn set_af(&mut self, v: bool) {
        self.set_flag(flags::AF, v)
    }

    // Arithmetic flag update helpers

    /// Update ZF, SF, PF from an 8-bit result.
    pub fn update_flags_u8(&mut self, result: u8) {
        self.set_zf(result == 0);
        self.set_sf(result & 0x80 != 0);
        self.set_pf(result.count_ones() % 2 == 0);
    }

    /// Update ZF, SF, PF from a 16-bit result.
    pub fn update_flags_u16(&mut self, result: u16) {
        self.set_zf(result == 0);
        self.set_sf(result & 0x8000 != 0);
        self.set_pf((result as u8).count_ones() % 2 == 0);
    }

    /// Update ZF, SF, PF from a 32-bit result.
    #[allow(dead_code)]
    pub fn update_flags_u32(&mut self, result: u32) {
        self.set_zf(result == 0);
        self.set_sf(result & 0x80000000 != 0);
        self.set_pf((result as u8).count_ones() % 2 == 0);
    }

    // Debug dump

    pub fn dump(&self) -> String {
        format!(
            "EAX={:08X} EBX={:08X} ECX={:08X} EDX={:08X} \
             ESI={:08X} EDI={:08X} EBP={:08X} ESP={:08X} \
             CS={:04X} DS={:04X} ES={:04X} SS={:04X} FS={:04X} GS={:04X} \
             EIP={:08X} EFLAGS={:08X} [{}{}{}{}{}{}{}]",
            self.gp[0],
            self.gp[3],
            self.gp[1],
            self.gp[2],
            self.gp[6],
            self.gp[7],
            self.gp[5],
            self.gp[4],
            self.get_seg(SegReg::CS),
            self.get_seg(SegReg::DS),
            self.get_seg(SegReg::ES),
            self.get_seg(SegReg::SS),
            self.get_seg(SegReg::FS),
            self.get_seg(SegReg::GS),
            self.ip,
            self.flags,
            if self.get_cf() { 'C' } else { '-' },
            if self.get_zf() { 'Z' } else { '-' },
            if self.get_sf() { 'S' } else { '-' },
            if self.get_of() { 'O' } else { '-' },
            if self.get_pf() { 'P' } else { '-' },
            if self.get_af() { 'A' } else { '-' },
            if self.get_df() { 'D' } else { '-' },
        )
    }
}

impl Default for Regs {
    fn default() -> Self {
        Self::new()
    }
}
