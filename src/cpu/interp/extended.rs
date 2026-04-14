use anyhow::Result;
use crate::bus::Bus;
use crate::cpu::regs::{};
use super::{Interpreter, StepResult};
use super::modrm::ModRm;

impl Interpreter {
    pub fn dispatch_0f(&mut self, bus: &mut Bus) -> Result<StepResult> {
        let opcode = self.fetch_u8(bus);
        match opcode {
            // IMUL r16/32, r/m16/32
            0xAF => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                if self.prefix.operand_32 {
                    let src = self.read_modrm_u32(&modrm, bus) as i32;
                    let dst = self.regs.get32(Self::reg16_from_field(modrm.reg)) as i32;
                    let (res, over) = dst.overflowing_mul(src);
                    self.regs.set32(Self::reg16_from_field(modrm.reg), res as u32);
                    self.regs.set_cf(over);
                    self.regs.set_of(over);
                } else {
                    let src = self.read_modrm_u16(&modrm, bus) as i16;
                    let dst = self.regs.get16(Self::reg16_from_field(modrm.reg)) as i16;
                    let (res, over) = dst.overflowing_mul(src);
                    self.regs.set16(Self::reg16_from_field(modrm.reg), res as u16);
                    self.regs.set_cf(over);
                    self.regs.set_of(over);
                }
                Ok(StepResult::Continue)
            }
            // MOVSX r16/32, r/m8
            0xBE => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let val = self.read_modrm_u8(&modrm, bus) as i8 as i32;
                if self.prefix.operand_32 {
                    self.regs.set32(Self::reg16_from_field(modrm.reg), val as u32);
                } else {
                    self.regs.set16(Self::reg16_from_field(modrm.reg), val as u16);
                }
                Ok(StepResult::Continue)
            }
            // MOVSX r16/32, r/m16
            0xBF => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let val = self.read_modrm_u16(&modrm, bus) as i16 as i32;
                if self.prefix.operand_32 {
                    self.regs.set32(Self::reg16_from_field(modrm.reg), val as u32);
                } else {
                    self.regs.set16(Self::reg16_from_field(modrm.reg), val as u16);
                }
                Ok(StepResult::Continue)
            }
            // MOVZX r16/32, r/m8
            0xB6 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let val = self.read_modrm_u8(&modrm, bus) as u32;
                if self.prefix.operand_32 {
                    self.regs.set32(Self::reg16_from_field(modrm.reg), val);
                } else {
                    self.regs.set16(Self::reg16_from_field(modrm.reg), val as u16);
                }
                Ok(StepResult::Continue)
            }
            // MOVZX r16/32, r/m16
            0xB7 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let val = self.read_modrm_u16(&modrm, bus) as u32;
                if self.prefix.operand_32 {
                    self.regs.set32(Self::reg16_from_field(modrm.reg), val);
                } else {
                    self.regs.set16(Self::reg16_from_field(modrm.reg), val as u16);
                }
                Ok(StepResult::Continue)
            }
            // Jcc near (0x80..=0x8F) - relative 16/32-bit
            0x80..=0x8F => {
                let cond = opcode & 0x0F;
                let rel = if self.prefix.operand_32 {
                    self.fetch_i32(bus) as i32
                } else {
                    self.fetch_i16(bus) as i32
                };
                if self.check_cond(cond) {
                    self.regs.ip = self.regs.ip.wrapping_add(rel as u32);
                }
                Ok(StepResult::Continue)
            }
            _ => {
                log::warn!("Unimplemented 0x0F extended opcode: {:02X}", opcode);
                Ok(StepResult::Unimplemented(opcode))
            }
        }
    }

    pub fn check_cond(&self, cond: u8) -> bool {
        match cond {
            0x0 => self.regs.get_of(),         // JO
            0x1 => !self.regs.get_of(),        // JNO
            0x2 => self.regs.get_cf(),         // JB/JC/JNAE
            0x3 => !self.regs.get_cf(),        // JNB/JNC/JAE
            0x4 => self.regs.get_zf(),         // JE/JZ
            0x5 => !self.regs.get_zf(),        // JNE/JNZ
            0x6 => self.regs.get_cf() || self.regs.get_zf(), // JBE/JNA
            0x7 => !self.regs.get_cf() && !self.regs.get_zf(), // JNBE/JA
            0x8 => self.regs.get_sf(),         // JS
            0x9 => !self.regs.get_sf(),        // JNS
            0xA => self.regs.get_pf(),         // JP/JPE
            0xB => !self.regs.get_pf(),        // JNP/JPO
            0xC => self.regs.get_sf() != self.regs.get_of(), // JL/JNGE
            0xD => self.regs.get_sf() == self.regs.get_of(), // JNL/JGE
            0xE => self.regs.get_zf() || (self.regs.get_sf() != self.regs.get_of()), // JLE/JNG
            0xF => !self.regs.get_zf() && (self.regs.get_sf() == self.regs.get_of()), // JNLE/JG
            _ => false,
        }
    }
}
