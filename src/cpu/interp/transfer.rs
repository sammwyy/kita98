use crate::bus::Bus;
use crate::cpu::regs::{Reg16, Reg8, SegReg};
use super::{Interpreter, StepResult};
use super::modrm::ModRm;
use anyhow::Result;

impl Interpreter {
    pub fn dispatch_transfer(&mut self, opcode: u8, bus: &mut Bus, ip_before: u32) -> Result<StepResult> {
        match opcode {
            // MOV r8, imm8
            0xB0..=0xB7 => {
                let reg = Self::reg8_from_field(opcode & 7);
                let imm = self.fetch_u8(bus);
                self.regs.set8(reg, imm);
                Ok(StepResult::Continue)
            }
            // MOV r16, imm16
            0xB8..=0xBF => {
                let reg = Self::reg16_from_field(opcode & 7);
                let imm = self.fetch_u16(bus);
                self.regs.set16(reg, imm);
                Ok(StepResult::Continue)
            }
            // MOV r/m, r
            0x88 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let val = self.regs.get8(Self::reg8_from_field(modrm.reg));
                self.write_modrm_u8(&modrm, bus, val);
                Ok(StepResult::Continue)
            }
            0x89 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let val = self.regs.get16(Self::reg16_from_field(modrm.reg));
                self.write_modrm_u16(&modrm, bus, val);
                Ok(StepResult::Continue)
            }
            // MOV r, r/m
            0x8A => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let val = self.read_modrm_u8(&modrm, bus);
                self.regs.set8(Self::reg8_from_field(modrm.reg), val);
                Ok(StepResult::Continue)
            }
            0x8B => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let val = self.read_modrm_u16(&modrm, bus);
                self.regs.set16(Self::reg16_from_field(modrm.reg), val);
                Ok(StepResult::Continue)
            }
            // MOV r/m, imm
            0xC6 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let imm = self.fetch_u8(bus);
                self.write_modrm_u8(&modrm, bus, imm);
                Ok(StepResult::Continue)
            }
            0xC7 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let imm = self.fetch_u16(bus);
                self.write_modrm_u16(&modrm, bus, imm);
                Ok(StepResult::Continue)
            }
            // MOVS to/from AL/AX
            0xA0 => {
                let off = self.fetch_u16(bus) as u32;
                let seg = self.prefix.seg_override.unwrap_or(SegReg::DS);
                let v = bus.mem.seg_read_u8(self.regs.get_seg(seg), off);
                self.regs.set8(Reg8::AL, v);
                Ok(StepResult::Continue)
            }
            0xA1 => {
                let off = self.fetch_u16(bus) as u32;
                let seg = self.prefix.seg_override.unwrap_or(SegReg::DS);
                let v = bus.mem.seg_read_u16(self.regs.get_seg(seg), off);
                self.regs.set16(Reg16::AX, v);
                Ok(StepResult::Continue)
            }
            0xA2 => {
                let off = self.fetch_u16(bus) as u32;
                let seg = self.prefix.seg_override.unwrap_or(SegReg::DS);
                let v = self.regs.get8(Reg8::AL);
                bus.mem.seg_write_u8(self.regs.get_seg(seg), off, v);
                Ok(StepResult::Continue)
            }
            0xA3 => {
                let off = self.fetch_u16(bus) as u32;
                let seg = self.prefix.seg_override.unwrap_or(SegReg::DS);
                let v = self.regs.get16(Reg16::AX);
                bus.mem.seg_write_u16(self.regs.get_seg(seg), off, v);
                Ok(StepResult::Continue)
            }

            // LEA
            0x8D => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let ea = self.ea_from_modrm(&modrm, bus);
                self.regs.set16(Self::reg16_from_field(modrm.reg), ea as u16);
                Ok(StepResult::Continue)
            }

            // XCHG
            0x90..=0x97 => {
                if opcode == 0x90 { return Ok(StepResult::Continue); } // NOP
                let reg = Self::reg16_from_field(opcode & 7);
                let a = self.regs.get16(Reg16::AX);
                let b = self.regs.get16(reg);
                self.regs.set16(Reg16::AX, b);
                self.regs.set16(reg, a);
                Ok(StepResult::Continue)
            }
            0x86 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let a = self.regs.get8(Self::reg8_from_field(modrm.reg));
                let b = self.read_modrm_u8(&modrm, bus);
                self.regs.set8(Self::reg8_from_field(modrm.reg), b);
                self.write_modrm_u8(&modrm, bus, a);
                Ok(StepResult::Continue)
            }
            0x87 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let a = self.regs.get16(Self::reg16_from_field(modrm.reg));
                let b = self.read_modrm_u16(&modrm, bus);
                self.regs.set16(Self::reg16_from_field(modrm.reg), b);
                self.write_modrm_u16(&modrm, bus, a);
                Ok(StepResult::Continue)
            }

            // Segment loads
            0xC4 => { // LES
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let ea = self.ea_from_modrm(&modrm, bus);
                let off = bus.mem.read_u16(ea);
                let seg = bus.mem.read_u16(ea + 2);
                self.regs.set16(Self::reg16_from_field(modrm.reg), off);
                self.regs.set_seg(SegReg::ES, seg);
                Ok(StepResult::Continue)
            }
            0xC5 => { // LDS
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let ea = self.ea_from_modrm(&modrm, bus);
                let off = bus.mem.read_u16(ea);
                let seg = bus.mem.read_u16(ea + 2);
                self.regs.set16(Self::reg16_from_field(modrm.reg), off);
                self.regs.set_seg(SegReg::DS, seg);
                Ok(StepResult::Continue)
            }

            // POP r/m16 (8F)
            0x8F => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let val = self.pop16(bus);
                self.write_modrm_u16(&modrm, bus, val);
                Ok(StepResult::Continue)
            }

            _ => self.dispatch_flow(opcode, bus, ip_before),
        }
    }
}
