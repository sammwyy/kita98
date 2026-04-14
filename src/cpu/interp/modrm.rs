use crate::cpu::regs::{Reg16, Reg8, SegReg};
use crate::bus::Bus;
use crate::memory::Memory;
use super::Interpreter;

/// Decoded ModRM byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModRm {
    pub mode: u8, // 0-3
    pub reg: u8,  // register field (3 bits)
    pub rm: u8,   // r/m field (3 bits)
}

impl ModRm {
    pub fn decode(byte: u8) -> Self {
        Self {
            mode: (byte >> 6) & 3,
            reg: (byte >> 3) & 7,
            rm: byte & 7,
        }
    }
}

impl Interpreter {
    /// Returns the (segment, offset) described by a ModRM byte.
    pub fn decode_ea(&mut self, modrm: &ModRm, bus: &Bus) -> (SegReg, u32) {
        if self.prefix.address_32 {
            self.decode_ea_32(modrm, bus)
        } else {
            let (seg, off) = self.decode_ea_16(modrm, bus);
            (seg, off as u32)
        }
    }

    pub fn decode_ea_16(&mut self, modrm: &ModRm, bus: &Bus) -> (SegReg, u16) {
        // Default segment selection (8086)
        let default_seg = match modrm.rm {
            2 | 3 => SegReg::SS,
            6 => if modrm.mode != 0 { SegReg::SS } else { SegReg::DS },
            _ => SegReg::DS,
        };
        let seg = self.prefix.seg_override.unwrap_or(default_seg);

        let base: u16 = match modrm.rm {
            0 => self.regs.get16(Reg16::BX).wrapping_add(self.regs.get16(Reg16::SI)),
            1 => self.regs.get16(Reg16::BX).wrapping_add(self.regs.get16(Reg16::DI)),
            2 => self.regs.get16(Reg16::BP).wrapping_add(self.regs.get16(Reg16::SI)),
            3 => self.regs.get16(Reg16::BP).wrapping_add(self.regs.get16(Reg16::DI)),
            4 => self.regs.get16(Reg16::SI),
            5 => self.regs.get16(Reg16::DI),
            6 => {
                if modrm.mode == 0 {
                    self.fetch_u16(bus)
                } else {
                    self.regs.get16(Reg16::BP)
                }
            }
            7 => self.regs.get16(Reg16::BX),
            _ => unreachable!(),
        };

        let offset = match modrm.mode {
            0 => base,
            1 => base.wrapping_add(self.fetch_i8(bus) as u16),
            2 => base.wrapping_add(self.fetch_i16(bus) as u16),
            _ => unreachable!("mode==3 has no EA"),
        };

        (seg, offset)
    }

    pub fn decode_ea_32(&mut self, modrm: &ModRm, bus: &Bus) -> (SegReg, u32) {
        let mut default_seg = SegReg::DS;
        let mut offset: u32;

        if modrm.rm == 4 {
            // SIB byte
            let sib = self.fetch_u8(bus);
            let ss = (sib >> 6) & 3;
            let index = (sib >> 3) & 7;
            let base = sib & 7;

            let mut val = if base == 5 && modrm.mode == 0 {
                self.fetch_u32(bus)
            } else {
                if base == 4 || base == 5 { default_seg = SegReg::SS; }
                self.regs.get32(Self::reg16_from_field(base))
            };

            if index != 4 {
                let idx_val = self.regs.get32(Self::reg16_from_field(index));
                val = val.wrapping_add(idx_val << ss);
            }
            offset = val;
        } else if modrm.rm == 5 && modrm.mode == 0 {
            offset = self.fetch_u32(bus);
        } else {
            let base_reg = Self::reg16_from_field(modrm.rm);
            if modrm.rm == 4 || modrm.rm == 5 { default_seg = SegReg::SS; }
            offset = self.regs.get32(base_reg);
        }

        match modrm.mode {
            1 => offset = offset.wrapping_add(self.fetch_i8(bus) as u32),
            2 => offset = offset.wrapping_add(self.fetch_u32(bus)),
            _ => {}
        }

        let seg = self.prefix.seg_override.unwrap_or(default_seg);
        (seg, offset)
    }

    pub fn ea_from_modrm(&mut self, modrm: &ModRm, bus: &Bus) -> u32 {
        let (seg, offset) = self.decode_ea(modrm, bus);
        Memory::phys(self.regs.get_seg(seg), offset)
    }

    pub fn reg16_from_field(field: u8) -> Reg16 {
        match field {
            0 => Reg16::AX,
            1 => Reg16::CX,
            2 => Reg16::DX,
            3 => Reg16::BX,
            4 => Reg16::SP,
            5 => Reg16::BP,
            6 => Reg16::SI,
            7 => Reg16::DI,
            _ => unreachable!(),
        }
    }

    pub fn reg8_from_field(field: u8) -> Reg8 {
        match field {
            0 => Reg8::AL,
            1 => Reg8::CL,
            2 => Reg8::DL,
            3 => Reg8::BL,
            4 => Reg8::AH,
            5 => Reg8::CH,
            6 => Reg8::DH,
            7 => Reg8::BH,
            _ => unreachable!(),
        }
    }

    pub fn seg_from_field(field: u8) -> SegReg {
        match field & 7 {
            0 => SegReg::ES,
            1 => SegReg::CS,
            2 => SegReg::SS,
            3 => SegReg::DS,
            4 => SegReg::FS,
            5 => SegReg::GS,
            _ => SegReg::DS,
        }
    }

    pub fn read_modrm_u8(&mut self, modrm: &ModRm, bus: &Bus) -> u8 {
        if modrm.mode == 3 {
            self.regs.get8(Self::reg8_from_field(modrm.rm))
        } else {
            let ea = self.ea_from_modrm(modrm, bus);
            bus.mem.read_u8(ea)
        }
    }

    pub fn write_modrm_u8(&mut self, modrm: &ModRm, bus: &mut Bus, val: u8) {
        if modrm.mode == 3 {
            self.regs.set8(Self::reg8_from_field(modrm.rm), val);
        } else {
            let ea = self.ea_from_modrm(modrm, bus);
            bus.mem.write_u8(ea, val);
        }
    }

    pub fn read_modrm_u16(&mut self, modrm: &ModRm, bus: &Bus) -> u16 {
        if modrm.mode == 3 {
            self.regs.get16(Self::reg16_from_field(modrm.rm))
        } else {
            let ea = self.ea_from_modrm(modrm, bus);
            bus.mem.read_u16(ea)
        }
    }

    pub fn write_modrm_u16(&mut self, modrm: &ModRm, bus: &mut Bus, val: u16) {
        if modrm.mode == 3 {
            self.regs.set16(Self::reg16_from_field(modrm.rm), val);
        } else {
            let ea = self.ea_from_modrm(modrm, bus);
            bus.mem.write_u16(ea, val);
        }
    }

    pub fn read_modrm_u32(&mut self, modrm: &ModRm, bus: &Bus) -> u32 {
        if modrm.mode == 3 {
            self.regs.get32(Self::reg16_from_field(modrm.rm))
        } else {
            let ea = self.ea_from_modrm(modrm, bus);
            bus.mem.read_u32(ea)
        }
    }

    #[allow(dead_code)]
    pub fn write_modrm_u32(&mut self, modrm: &ModRm, bus: &mut Bus, val: u32) {
        if modrm.mode == 3 {
            self.regs.set32(Self::reg16_from_field(modrm.rm), val);
        } else {
            let ea = self.ea_from_modrm(modrm, bus);
            bus.mem.write_u32(ea, val);
        }
    }
}
