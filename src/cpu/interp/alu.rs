use crate::bus::Bus;
use crate::cpu::regs::{};
use super::Interpreter;
use super::modrm::ModRm;

#[allow(dead_code)]
impl Interpreter {
    pub fn update_add8(&mut self, a: u8, b: u8, result: u8) {
        self.regs.update_flags_u8(result);
        self.regs.set_cf((result as u16) < (a as u16) + (b as u16));
        self.regs.set_of(((a ^ result) & (b ^ result) & 0x80) != 0);
    }

    pub fn update_add16(&mut self, a: u16, b: u16, result: u16) {
        self.regs.update_flags_u16(result);
        self.regs.set_cf((result as u32) < (a as u32) + (b as u32));
        self.regs.set_of(((a ^ result) & (b ^ result) & 0x8000) != 0);
    }

    pub fn update_sub8(&mut self, a: u8, b: u8, result: u8) {
        self.regs.update_flags_u8(result);
        self.regs.set_cf((a as u16) < (b as u16));
        self.regs.set_of(((a ^ b) & (a ^ result) & 0x80) != 0);
    }

    pub fn update_sub16(&mut self, a: u16, b: u16, result: u16) {
        self.regs.update_flags_u16(result);
        self.regs.set_cf(a < b);
        self.regs.set_of(((a ^ b) & (a ^ result) & 0x8000) != 0);
    }

    pub fn update_add32(&mut self, a: u32, b: u32, result: u32) {
        self.regs.update_flags_u32(result);
        self.regs.set_cf(result < a);
        self.regs.set_of(((a ^ result) & (b ^ result) & 0x80000000) != 0);
    }

    pub fn update_sub32(&mut self, a: u32, b: u32, result: u32) {
        self.regs.update_flags_u32(result);
        self.regs.set_cf(a < b);
        self.regs.set_of(((a ^ b) & (a ^ result) & 0x80000000) != 0);
    }

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

    pub fn alu_rmr8(&mut self, bus: &mut Bus, op: fn(u8, u8) -> u8, affect_cf: bool) {
        let mb = self.fetch_u8(bus);
        let modrm = ModRm::decode(mb);
        let s = self.regs.get8(Self::reg8_from_field(modrm.reg));
        let d = self.read_modrm_u8(&modrm, bus);
        let r = op(d, s);
        self.regs.update_flags_u8(r);
        if !affect_cf {
            self.regs.set_cf(false);
            self.regs.set_of(false);
        }
        self.write_modrm_u8(&modrm, bus, r);
    }

    pub fn alu_rmr16(&mut self, bus: &mut Bus, op: fn(u16, u16) -> u16, affect_cf: bool) {
        let mb = self.fetch_u8(bus);
        let modrm = ModRm::decode(mb);
        let s = self.regs.get16(Self::reg16_from_field(modrm.reg));
        let d = self.read_modrm_u16(&modrm, bus);
        let r = op(d, s);
        self.regs.update_flags_u16(r);
        if !affect_cf {
            self.regs.set_cf(false);
            self.regs.set_of(false);
        }
        self.write_modrm_u16(&modrm, bus, r);
    }

    pub fn alu_rrm8(&mut self, bus: &mut Bus, op: fn(u8, u8) -> u8, affect_cf: bool) {
        let mb = self.fetch_u8(bus);
        let modrm = ModRm::decode(mb);
        let d = self.regs.get8(Self::reg8_from_field(modrm.reg));
        let s = self.read_modrm_u8(&modrm, bus);
        let r = op(d, s);
        self.regs.update_flags_u8(r);
        if !affect_cf {
            self.regs.set_cf(false);
            self.regs.set_of(false);
        }
        self.regs.set8(Self::reg8_from_field(modrm.reg), r);
    }

    pub fn alu_rrm16(&mut self, bus: &mut Bus, op: fn(u16, u16) -> u16, affect_cf: bool) {
        let mb = self.fetch_u8(bus);
        let modrm = ModRm::decode(mb);
        let d = self.regs.get16(Self::reg16_from_field(modrm.reg));
        let s = self.read_modrm_u16(&modrm, bus);
        let r = op(d, s);
        self.regs.update_flags_u16(r);
        if !affect_cf {
            self.regs.set_cf(false);
            self.regs.set_of(false);
        }
        self.regs.set16(Self::reg16_from_field(modrm.reg), r);
    }

    pub fn shift_rm8(&mut self, bus: &mut Bus, mb: u8, count: u8) {
        let modrm = ModRm::decode(mb);
        let v = self.read_modrm_u8(&modrm, bus);
        let cnt = (count & 0x1F) as u32;
        if cnt == 0 {
            return;
        }
        let r = match modrm.reg {
            0 => {
                let c = v.checked_shl(cnt).unwrap_or(0);
                self.regs.set_cf((v >> (8 - cnt.min(8))) & 1 != 0);
                self.regs.set_of((c ^ v) & 0x80 != 0);
                c
            }
            1 => {
                let c = v.checked_shr(cnt).unwrap_or(0);
                self.regs.set_cf((v >> (cnt - 1).min(7)) & 1 != 0);
                c
            }
            4 => {
                let c = v.checked_shl(cnt).unwrap_or(0);
                self.regs.set_cf((v >> (8 - cnt.min(8))) & 1 != 0);
                c
            }
            5 => {
                let c = v.checked_shr(cnt).unwrap_or(0);
                self.regs.set_cf((v >> (cnt - 1).min(7)) & 1 != 0);
                c
            }
            7 => {
                let sv = v as i8;
                let c = sv.checked_shr(cnt).unwrap_or(if sv < 0 { -1 } else { 0 }) as u8;
                c
            }
            _ => {
                log::warn!("Unhandled shift grp2/r={}", modrm.reg);
                v
            }
        };
        self.regs.update_flags_u8(r);
        self.write_modrm_u8(&modrm, bus, r);
    }

    pub fn shift_rm16(&mut self, bus: &mut Bus, mb: u8, count: u8) {
        let modrm = ModRm::decode(mb);
        let v = self.read_modrm_u16(&modrm, bus);
        let cnt = (count & 0x1F) as u32;
        if cnt == 0 {
            return;
        }
        let r = match modrm.reg {
            0 => {
                let c = v.checked_shl(cnt).unwrap_or(0);
                self.regs.set_cf((v >> (16 - cnt.min(16))) & 1 != 0);
                c
            }
            1 => {
                let c = v.checked_shr(cnt).unwrap_or(0);
                self.regs.set_cf((v >> (cnt - 1).min(15)) & 1 != 0);
                c
            }
            4 => {
                let c = v.checked_shl(cnt).unwrap_or(0);
                self.regs.set_cf((v >> (16 - cnt.min(16))) & 1 != 0);
                c
            }
            5 => {
                let c = v.checked_shr(cnt).unwrap_or(0);
                self.regs.set_cf((v >> (cnt - 1).min(15)) & 1 != 0);
                c
            }
            7 => {
                let sv = v as i16;
                let c = sv.checked_shr(cnt).unwrap_or(if sv < 0 { -1 } else { 0 }) as u16;
                c
            }
            _ => {
                log::warn!("Unhandled shift grp2/r={}", modrm.reg);
                v
            }
        };
        self.regs.update_flags_u16(r);
        self.write_modrm_u16(&modrm, bus, r);
    }
}
