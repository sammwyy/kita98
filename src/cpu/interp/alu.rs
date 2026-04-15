use crate::bus::Bus;
use crate::cpu::regs::{Reg16, Reg8};
use super::{Interpreter, StepResult};
use super::modrm::ModRm;
use anyhow::Result;

impl Interpreter {
    pub fn group1_u8(&mut self, bus: &mut Bus, modrm: &ModRm, imm: u8) -> u8 {
        let dst = self.read_modrm_u8(modrm, bus);
        match modrm.reg {
            0 => { let r = dst.wrapping_add(imm); self.update_add8(dst, imm, r); r }
            1 => { let r = dst | imm; self.regs.update_flags_u8(r); self.regs.set_cf(false); self.regs.set_of(false); r }
            2 => {
                let cf = self.regs.get_cf() as u8;
                let r = dst.wrapping_add(imm).wrapping_add(cf);
                self.update_add8(dst, imm.wrapping_add(cf), r);
                r
            }
            3 => {
                let cf = self.regs.get_cf() as u8;
                let r = dst.wrapping_sub(imm).wrapping_sub(cf);
                self.update_sub8(dst, imm.wrapping_add(cf), r);
                r
            }
            4 => { let r = dst & imm; self.regs.update_flags_u8(r); self.regs.set_cf(false); self.regs.set_of(false); r }
            5 => { let r = dst.wrapping_sub(imm); self.update_sub8(dst, imm, r); r }
            6 => { let r = dst ^ imm; self.regs.update_flags_u8(r); self.regs.set_cf(false); self.regs.set_of(false); r }
            7 => { let r = dst.wrapping_sub(imm); self.update_sub8(dst, imm, r); dst }
            _ => unreachable!(),
        }
    }

    pub fn group1_u16(&mut self, bus: &mut Bus, modrm: &ModRm, imm: u16) -> u16 {
        let dst = self.read_modrm_u16(modrm, bus);
        match modrm.reg {
            0 => { let r = dst.wrapping_add(imm); self.update_add16(dst, imm, r); r }
            1 => { let r = dst | imm; self.regs.update_flags_u16(r); self.regs.set_cf(false); self.regs.set_of(false); r }
            2 => {
                let cf = self.regs.get_cf() as u16;
                let r = dst.wrapping_add(imm).wrapping_add(cf);
                self.update_add16(dst, imm.wrapping_add(cf), r);
                r
            }
            3 => {
                let cf = self.regs.get_cf() as u16;
                let r = dst.wrapping_sub(imm).wrapping_sub(cf);
                self.update_sub16(dst, imm.wrapping_add(cf), r);
                r
            }
            4 => { let r = dst & imm; self.regs.update_flags_u16(r); self.regs.set_cf(false); self.regs.set_of(false); r }
            5 => { let r = dst.wrapping_sub(imm); self.update_sub16(dst, imm, r); r }
            6 => { let r = dst ^ imm; self.regs.update_flags_u16(r); self.regs.set_cf(false); self.regs.set_of(false); r }
            7 => { let r = dst.wrapping_sub(imm); self.update_sub16(dst, imm, r); dst }
            _ => unreachable!(),
        }
    }

    pub fn group1_u32(&mut self, bus: &mut Bus, modrm: &ModRm, imm: u32) -> u32 {
        let dst = self.read_modrm_u32(modrm, bus);
        match modrm.reg {
            0 => { let r = dst.wrapping_add(imm); self.update_add32(dst, imm, r); r }
            1 => { let r = dst | imm; self.regs.update_flags_u32(r); self.regs.set_cf(false); self.regs.set_of(false); r }
            2 => {
                let cf = self.regs.get_cf() as u32;
                let r = dst.wrapping_add(imm).wrapping_add(cf);
                self.update_add32(dst, imm.wrapping_add(cf), r);
                r
            }
            3 => {
                let cf = self.regs.get_cf() as u32;
                let r = dst.wrapping_sub(imm).wrapping_sub(cf);
                self.update_sub32(dst, imm.wrapping_add(cf), r);
                r
            }
            4 => { let r = dst & imm; self.regs.update_flags_u32(r); self.regs.set_cf(false); self.regs.set_of(false); r }
            5 => { let r = dst.wrapping_sub(imm); self.update_sub32(dst, imm, r); r }
            6 => { let r = dst ^ imm; self.regs.update_flags_u32(r); self.regs.set_cf(false); self.regs.set_of(false); r }
            7 => { let r = dst.wrapping_sub(imm); self.update_sub32(dst, imm, r); dst }
            _ => unreachable!(),
        }
    }

    pub fn shift_rm8(&mut self, bus: &mut Bus, mb: u8, count: u8) {
        let modrm = ModRm::decode(mb);
        let v = self.read_modrm_u8(&modrm, bus);
        let cnt = (count & 0x1F) as u32;
        if cnt == 0 { return; }
        let r = match modrm.reg {
            0 => { // ROL
                let c = v.rotate_left(cnt);
                self.regs.set_cf((v >> (8 - cnt)) & 1 != 0);
                c
            }
            1 => { // ROR
                let c = v.rotate_right(cnt);
                self.regs.set_cf((v >> (cnt - 1)) & 1 != 0);
                c
            }
            2 => { // RCL
                let mut c = v;
                for _ in 0..cnt {
                    let old_cf = self.regs.get_cf() as u8;
                    self.regs.set_cf((c & 0x80) != 0);
                    c = (c << 1) | old_cf;
                }
                c
            }
            3 => { // RCR
                let mut c = v;
                for _ in 0..cnt {
                    let old_cf = self.regs.get_cf() as u8;
                    self.regs.set_cf((c & 0x01) != 0);
                    c = (c >> 1) | (old_cf << 7);
                }
                c
            }
            4 => { // SHL
                let c = v << (cnt - 1);
                self.regs.set_cf((c & 0x80) != 0);
                v << cnt
            }
            5 => { // SHR
                let c = v >> (cnt - 1);
                self.regs.set_cf((c & 0x01) != 0);
                v >> cnt
            }
            7 => { // SAR
                let c = (v as i8) >> (cnt - 1);
                self.regs.set_cf((c & 0x01) != 0);
                ((v as i8) >> cnt) as u8
            }
            _ => v,
        };
        self.regs.update_flags_u8(r);
        self.write_modrm_u8(&modrm, bus, r);
    }

    pub fn shift_rm16(&mut self, bus: &mut Bus, mb: u8, count: u8) {
        let modrm = ModRm::decode(mb);
        let v = self.read_modrm_u16(&modrm, bus);
        let cnt = (count & 0x1F) as u32;
        if cnt == 0 { return; }
        let r = match modrm.reg {
            0 => v.rotate_left(cnt),
            1 => v.rotate_right(cnt),
            4 => v << cnt,
            5 => v >> cnt,
            7 => ((v as i16) >> cnt) as u16,
            _ => v,
        };
        self.regs.update_flags_u16(r);
        self.write_modrm_u16(&modrm, bus, r);
    }

    pub fn dispatch_alu(&mut self, opcode: u8, bus: &mut Bus, ip_before: u32) -> Result<StepResult> {
        match opcode {
            // Group 1: ADD/OR/ADC/SBB/AND/SUB/XOR/CMP r/m, imm
            0x80 | 0x82 => {
                let mb = self.fetch_u8(bus);
                let imm = self.fetch_u8(bus);
                self.group1_u8(bus, &ModRm::decode(mb), imm);
                Ok(StepResult::Continue)
            }
            0x81 => {
                let mb = self.fetch_u8(bus);
                let imm = self.fetch_u16(bus);
                self.group1_u16(bus, &ModRm::decode(mb), imm);
                Ok(StepResult::Continue)
            }
            0x83 => {
                let mb = self.fetch_u8(bus);
                let imm = self.fetch_i8(bus) as i16 as u16;
                self.group1_u16(bus, &ModRm::decode(mb), imm);
                Ok(StepResult::Continue)
            }

            // Group 3: TEST/NOT/NEG/MUL/IMUL/DIV/IDIV r/m
            0xF6 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                match modrm.reg {
                    0 | 1 => {
                        let imm = self.fetch_u8(bus);
                        let val = self.read_modrm_u8(&modrm, bus);
                        let r = val & imm;
                        self.regs.update_flags_u8(r);
                        self.regs.set_cf(false);
                        self.regs.set_of(false);
                    }
                    2 => {
                        let val = self.read_modrm_u8(&modrm, bus);
                        let r = (!val).wrapping_add(1);
                        self.update_sub8(0, val, r);
                        self.write_modrm_u8(&modrm, bus, r);
                    }
                    3 => {
                        let val = self.read_modrm_u8(&modrm, bus);
                        let r = !val;
                        self.write_modrm_u8(&modrm, bus, r);
                    }
                    4 => {
                        let al = self.regs.get8(Reg8::AL) as u16;
                        let src = self.read_modrm_u8(&modrm, bus) as u16;
                        let r = al * src;
                        self.regs.set16(Reg16::AX, r);
                        self.regs.set_cf(r > 0xFF);
                        self.regs.set_of(r > 0xFF);
                    }
                    5 => {
                        let al = self.regs.get8(Reg8::AL) as i8 as i16;
                        let src = self.read_modrm_u8(&modrm, bus) as i8 as i16;
                        let r = al * src;
                        self.regs.set16(Reg16::AX, r as u16);
                        self.regs.set_cf(r > 0x7F || r < -0x80);
                        self.regs.set_of(r > 0x7F || r < -0x80);
                    }
                    6 => {
                        let src = self.read_modrm_u8(&modrm, bus);
                        if src == 0 { return Ok(StepResult::Interrupt(0)); }
                        let ax = self.regs.get16(Reg16::AX);
                        self.regs.set8(Reg8::AL, (ax / src as u16) as u8);
                        self.regs.set8(Reg8::AH, (ax % src as u16) as u8);
                    }
                    7 => {
                        let src = self.read_modrm_u8(&modrm, bus) as i8 as i16;
                        if src == 0 { return Ok(StepResult::Interrupt(0)); }
                        let ax = self.regs.get16(Reg16::AX) as i16;
                        self.regs.set8(Reg8::AL, (ax / src) as u8);
                        self.regs.set8(Reg8::AH, (ax % src) as u8);
                    }
                    _ => unreachable!(),
                }
                Ok(StepResult::Continue)
            }
            0xF7 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                match modrm.reg {
                    0 | 1 => {
                        let imm = self.fetch_u16(bus);
                        let val = self.read_modrm_u16(&modrm, bus);
                        let r = val & imm;
                        self.regs.update_flags_u16(r);
                        self.regs.set_cf(false);
                        self.regs.set_of(false);
                    }
                    2 => {
                        let val = self.read_modrm_u16(&modrm, bus);
                        let r = (!val).wrapping_add(1);
                        self.update_sub16(0, val, r);
                        self.write_modrm_u16(&modrm, bus, r);
                    }
                    3 => {
                        let val = self.read_modrm_u16(&modrm, bus);
                        let r = !val;
                        self.write_modrm_u16(&modrm, bus, r);
                    }
                    4 => {
                        let ax = self.regs.get16(Reg16::AX) as u32;
                        let src = self.read_modrm_u16(&modrm, bus) as u32;
                        let r = ax * src;
                        self.regs.set16(Reg16::AX, (r & 0xFFFF) as u16);
                        self.regs.set16(Reg16::DX, (r >> 16) as u16);
                        self.regs.set_cf(r > 0xFFFF);
                        self.regs.set_of(r > 0xFFFF);
                    }
                    5 => {
                        let ax = self.regs.get16(Reg16::AX) as i16 as i32;
                        let src = self.read_modrm_u16(&modrm, bus) as i16 as i32;
                        let r = ax * src;
                        self.regs.set16(Reg16::AX, (r & 0xFFFF) as u16);
                        self.regs.set16(Reg16::DX, (r >> 16) as u16);
                        self.regs.set_cf(r > 32767 || r < -32768);
                        self.regs.set_of(r > 32767 || r < -32768);
                    }
                    6 => {
                        let src = self.read_modrm_u16(&modrm, bus) as u32;
                        if src == 0 { return Ok(StepResult::Interrupt(0)); }
                        let dxax = ((self.regs.get16(Reg16::DX) as u32) << 16) | self.regs.get16(Reg16::AX) as u32;
                        self.regs.set16(Reg16::AX, (dxax / src) as u16);
                        self.regs.set16(Reg16::DX, (dxax % src) as u16);
                    }
                    7 => {
                        let src = self.read_modrm_u16(&modrm, bus) as i16 as i32;
                        if src == 0 { return Ok(StepResult::Interrupt(0)); }
                        let dxax = ((self.regs.get16(Reg16::DX) as i16 as i32) << 16) | self.regs.get16(Reg16::AX) as i16 as i32;
                        self.regs.set16(Reg16::AX, (dxax / src) as u16);
                        self.regs.set16(Reg16::DX, (dxax % src) as u16);
                    }
                    _ => unreachable!(),
                }
                Ok(StepResult::Continue)
            }

            _ => self.dispatch_flow(opcode, bus, ip_before),
        }
    }
}
