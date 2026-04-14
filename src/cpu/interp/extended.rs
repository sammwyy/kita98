use super::modrm::ModRm;
use super::{Interpreter, StepResult};
use crate::bus::Bus;
use crate::cpu::regs::Reg16;
use anyhow::Result;

impl Interpreter {
    pub fn dispatch_0f(&mut self, bus: &mut Bus) -> Result<StepResult> {
        let opcode = self.fetch_u8(bus);
        match opcode {
            // Group 6: SLDT/STR/LLDT/LTR/VERR/VERW
            // These are all system instructions; stub them out by consuming
            // the ModRM and returning zero / doing nothing.
            0x00 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                match modrm.reg {
                    0 => {
                        // SLDT r/m16 – store 0 (no LDT in real mode)
                        if modrm.mode != 3 {
                            self.decode_ea(&modrm, bus);
                        }
                        self.write_modrm_u16(&modrm, bus, 0);
                    }
                    1 => {
                        // STR r/m16 – store 0
                        if modrm.mode != 3 {
                            self.decode_ea(&modrm, bus);
                        }
                        self.write_modrm_u16(&modrm, bus, 0);
                    }
                    2 | 3 | 4 | 5 => {
                        // LLDT / LTR / VERR / VERW – ignored
                        if modrm.mode != 3 {
                            self.decode_ea(&modrm, bus);
                        }
                    }
                    _ => {}
                }
                Ok(StepResult::Continue)
            }

            // Group 7: SGDT/SIDT/LGDT/LIDT/SMSW/LMSW/INVLPG
            0x01 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                // All of these deal with descriptor tables / control regs.
                // Consume the EA and ignore.
                if modrm.mode != 3 {
                    self.decode_ea(&modrm, bus);
                }
                Ok(StepResult::Continue)
            }

            // LAR / LSL
            0x02 | 0x03 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                if modrm.mode != 3 {
                    self.decode_ea(&modrm, bus);
                }
                // Return 0 and set ZF=0 (access denied / not present)
                self.regs.set16(Self::reg16_from_field(modrm.reg), 0);
                self.regs.set_zf(false);
                Ok(StepResult::Continue)
            }

            // CLTS
            0x06 => Ok(StepResult::Continue),

            // INVD / WBINVD
            0x08 | 0x09 => Ok(StepResult::Continue),

            // UD2
            0x0B => Ok(StepResult::Interrupt(6)),

            // WRMSR / RDTSC / RDMSR / RDPMC
            0x30 => Ok(StepResult::Continue), // WRMSR
            0x31 => {
                self.regs.set16(Reg16::AX, 0);
                self.regs.set16(Reg16::DX, 0);
                Ok(StepResult::Continue)
            } // RDTSC
            0x32 => {
                self.regs.set16(Reg16::AX, 0);
                self.regs.set16(Reg16::DX, 0);
                Ok(StepResult::Continue)
            } // RDMSR
            0x33 => {
                self.regs.set16(Reg16::AX, 0);
                self.regs.set16(Reg16::DX, 0);
                Ok(StepResult::Continue)
            } // RDPMC

            // MOV CRn / DRn (0x20-0x23)
            0x20..=0x23 => {
                let _mb = self.fetch_u8(bus); // ModRM (always reg,reg form)
                Ok(StepResult::Continue)
            }

            // IMUL r16/32, r/m16/32 (0x0F AF)
            0xAF => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                if self.prefix.operand_32 {
                    let src = self.read_modrm_u32(&modrm, bus) as i32;
                    let dst = self.regs.get32(Self::reg16_from_field(modrm.reg)) as i32;
                    let (res, over) = dst.overflowing_mul(src);
                    self.regs
                        .set32(Self::reg16_from_field(modrm.reg), res as u32);
                    self.regs.set_cf(over);
                    self.regs.set_of(over);
                } else {
                    let src = self.read_modrm_u16(&modrm, bus) as i16;
                    let dst = self.regs.get16(Self::reg16_from_field(modrm.reg)) as i16;
                    let (res, over) = dst.overflowing_mul(src);
                    self.regs
                        .set16(Self::reg16_from_field(modrm.reg), res as u16);
                    self.regs.set_cf(over);
                    self.regs.set_of(over);
                }
                Ok(StepResult::Continue)
            }

            // Jcc near 16/32 (0x80-0x8F)
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

            // SETcc r/m8 (0x90-0x9F)
            0x90..=0x9F => {
                let cond = opcode & 0x0F;
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let val = if self.check_cond(cond) { 1u8 } else { 0u8 };
                self.write_modrm_u8(&modrm, bus, val);
                Ok(StepResult::Continue)
            }

            // PUSH FS / POP FS / CPUID / BT
            0xA0 => {
                let v = self.regs.get_seg(crate::cpu::regs::SegReg::FS);
                self.push16(bus, v);
                Ok(StepResult::Continue)
            }
            0xA1 => {
                let v = self.pop16(bus);
                self.regs.set_seg(crate::cpu::regs::SegReg::FS, v);
                Ok(StepResult::Continue)
            }
            0xA2 => {
                // CPUID – return zero / "no features"
                self.regs.set16(Reg16::AX, 0);
                self.regs.set16(Reg16::BX, 0);
                self.regs.set16(Reg16::CX, 0);
                self.regs.set16(Reg16::DX, 0);
                Ok(StepResult::Continue)
            }
            0xA3 => {
                // BT r/m16, r16 – bit test; set CF
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let base = self.read_modrm_u16(&modrm, bus);
                let bit = self.regs.get16(Self::reg16_from_field(modrm.reg)) & 15;
                self.regs.set_cf((base >> bit) & 1 != 0);
                Ok(StepResult::Continue)
            }
            0xA4 => {
                // SHLD r/m16, r16, imm8
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let src = self.regs.get16(Self::reg16_from_field(modrm.reg));
                let cnt = self.fetch_u8(bus) & 31;
                let dst = self.read_modrm_u16(&modrm, bus);
                if cnt == 0 {
                    return Ok(StepResult::Continue);
                }
                let wide = ((dst as u32) << 16) | src as u32;
                let r = (wide << cnt) >> 16;
                self.regs.set_cf((dst >> (16 - cnt)) & 1 != 0);
                self.regs.update_flags_u16(r as u16);
                self.write_modrm_u16(&modrm, bus, r as u16);
                Ok(StepResult::Continue)
            }
            0xA5 => {
                // SHLD r/m16, r16, CL
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let src = self.regs.get16(Self::reg16_from_field(modrm.reg));
                let cnt = self.regs.get8(crate::cpu::regs::Reg8::CL) & 31;
                let dst = self.read_modrm_u16(&modrm, bus);
                if cnt == 0 {
                    return Ok(StepResult::Continue);
                }
                let wide = ((dst as u32) << 16) | src as u32;
                let r = (wide << cnt) >> 16;
                self.regs.set_cf((dst >> (16 - cnt)) & 1 != 0);
                self.regs.update_flags_u16(r as u16);
                self.write_modrm_u16(&modrm, bus, r as u16);
                Ok(StepResult::Continue)
            }
            0xA8 => {
                let v = self.regs.get_seg(crate::cpu::regs::SegReg::GS);
                self.push16(bus, v);
                Ok(StepResult::Continue)
            }
            0xA9 => {
                let v = self.pop16(bus);
                self.regs.set_seg(crate::cpu::regs::SegReg::GS, v);
                Ok(StepResult::Continue)
            }
            0xAB => {
                // BTS r/m16, r16 – bit test and set
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let base = self.read_modrm_u16(&modrm, bus);
                let bit = self.regs.get16(Self::reg16_from_field(modrm.reg)) & 15;
                self.regs.set_cf((base >> bit) & 1 != 0);
                self.write_modrm_u16(&modrm, bus, base | (1 << bit));
                Ok(StepResult::Continue)
            }
            0xAC => {
                // SHRD r/m16, r16, imm8
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let src = self.regs.get16(Self::reg16_from_field(modrm.reg));
                let cnt = self.fetch_u8(bus) & 31;
                let dst = self.read_modrm_u16(&modrm, bus);
                if cnt == 0 {
                    return Ok(StepResult::Continue);
                }
                let wide = ((src as u32) << 16) | dst as u32;
                let r = (wide >> cnt) as u16;
                self.regs.set_cf((dst >> (cnt - 1)) & 1 != 0);
                self.regs.update_flags_u16(r);
                self.write_modrm_u16(&modrm, bus, r);
                Ok(StepResult::Continue)
            }
            0xAD => {
                // SHRD r/m16, r16, CL
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let src = self.regs.get16(Self::reg16_from_field(modrm.reg));
                let cnt = self.regs.get8(crate::cpu::regs::Reg8::CL) & 31;
                let dst = self.read_modrm_u16(&modrm, bus);
                if cnt == 0 {
                    return Ok(StepResult::Continue);
                }
                let wide = ((src as u32) << 16) | dst as u32;
                let r = (wide >> cnt) as u16;
                self.regs.set_cf((dst >> (cnt - 1)) & 1 != 0);
                self.regs.update_flags_u16(r);
                self.write_modrm_u16(&modrm, bus, r);
                Ok(StepResult::Continue)
            }

            // MOVZX
            0xB6 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let val = self.read_modrm_u8(&modrm, bus) as u32;
                if self.prefix.operand_32 {
                    self.regs.set32(Self::reg16_from_field(modrm.reg), val);
                } else {
                    self.regs
                        .set16(Self::reg16_from_field(modrm.reg), val as u16);
                }
                Ok(StepResult::Continue)
            }
            0xB7 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let val = self.read_modrm_u16(&modrm, bus) as u32;
                if self.prefix.operand_32 {
                    self.regs.set32(Self::reg16_from_field(modrm.reg), val);
                } else {
                    self.regs
                        .set16(Self::reg16_from_field(modrm.reg), val as u16);
                }
                Ok(StepResult::Continue)
            }

            // Group 8: BT/BTS/BTR/BTC r/m16, imm8 (0xBA)
            0xBA => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let base = self.read_modrm_u16(&modrm, bus);
                let bit = self.fetch_u8(bus) & 15;
                self.regs.set_cf((base >> bit) & 1 != 0);
                let result = match modrm.reg {
                    4 => base,               // BT  – test only
                    5 => base | (1 << bit),  // BTS – set
                    6 => base & !(1 << bit), // BTR – reset
                    7 => base ^ (1 << bit),  // BTC – complement
                    _ => base,
                };
                if modrm.reg >= 5 {
                    self.write_modrm_u16(&modrm, bus, result);
                }
                Ok(StepResult::Continue)
            }
            0xB3 => {
                // BTR r/m16, r16
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let base = self.read_modrm_u16(&modrm, bus);
                let bit = self.regs.get16(Self::reg16_from_field(modrm.reg)) & 15;
                self.regs.set_cf((base >> bit) & 1 != 0);
                self.write_modrm_u16(&modrm, bus, base & !(1 << bit));
                Ok(StepResult::Continue)
            }
            0xBB => {
                // BTC r/m16, r16
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let base = self.read_modrm_u16(&modrm, bus);
                let bit = self.regs.get16(Self::reg16_from_field(modrm.reg)) & 15;
                self.regs.set_cf((base >> bit) & 1 != 0);
                self.write_modrm_u16(&modrm, bus, base ^ (1 << bit));
                Ok(StepResult::Continue)
            }

            // BSF / BSR
            0xBC => {
                // BSF r16, r/m16
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let src = self.read_modrm_u16(&modrm, bus);
                if src == 0 {
                    self.regs.set_zf(true);
                } else {
                    let bit = src.trailing_zeros() as u16;
                    self.regs.set16(Self::reg16_from_field(modrm.reg), bit);
                    self.regs.set_zf(false);
                }
                Ok(StepResult::Continue)
            }
            0xBD => {
                // BSR r16, r/m16
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let src = self.read_modrm_u16(&modrm, bus);
                if src == 0 {
                    self.regs.set_zf(true);
                } else {
                    let bit = 15 - src.leading_zeros() as u16;
                    self.regs.set16(Self::reg16_from_field(modrm.reg), bit);
                    self.regs.set_zf(false);
                }
                Ok(StepResult::Continue)
            }

            // MOVSX
            0xBE => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let val = self.read_modrm_u8(&modrm, bus) as i8 as i32;
                if self.prefix.operand_32 {
                    self.regs
                        .set32(Self::reg16_from_field(modrm.reg), val as u32);
                } else {
                    self.regs
                        .set16(Self::reg16_from_field(modrm.reg), val as u16);
                }
                Ok(StepResult::Continue)
            }
            0xBF => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let val = self.read_modrm_u16(&modrm, bus) as i16 as i32;
                if self.prefix.operand_32 {
                    self.regs
                        .set32(Self::reg16_from_field(modrm.reg), val as u32);
                } else {
                    self.regs
                        .set16(Self::reg16_from_field(modrm.reg), val as u16);
                }
                Ok(StepResult::Continue)
            }

            // XADD
            0xC0 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let reg = Self::reg8_from_field(modrm.reg);
                let src = self.regs.get8(reg);
                let dst = self.read_modrm_u8(&modrm, bus);
                let sum = dst.wrapping_add(src);
                self.update_add8(dst, src, sum);
                self.regs.set8(reg, dst);
                self.write_modrm_u8(&modrm, bus, sum);
                Ok(StepResult::Continue)
            }
            0xC1 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let reg = Self::reg16_from_field(modrm.reg);
                let src = self.regs.get16(reg);
                let dst = self.read_modrm_u16(&modrm, bus);
                let sum = dst.wrapping_add(src);
                self.update_add16(dst, src, sum);
                self.regs.set16(reg, dst);
                self.write_modrm_u16(&modrm, bus, sum);
                Ok(StepResult::Continue)
            }

            _ => {
                log::warn!("Unimplemented 0x0F opcode: {:02X}", opcode);
                Ok(StepResult::Continue) // continue instead of returning Unimplemented
            }
        }
    }

    pub fn check_cond(&self, cond: u8) -> bool {
        match cond {
            0x0 => self.regs.get_of(),
            0x1 => !self.regs.get_of(),
            0x2 => self.regs.get_cf(),
            0x3 => !self.regs.get_cf(),
            0x4 => self.regs.get_zf(),
            0x5 => !self.regs.get_zf(),
            0x6 => self.regs.get_cf() || self.regs.get_zf(),
            0x7 => !self.regs.get_cf() && !self.regs.get_zf(),
            0x8 => self.regs.get_sf(),
            0x9 => !self.regs.get_sf(),
            0xA => self.regs.get_pf(),
            0xB => !self.regs.get_pf(),
            0xC => self.regs.get_sf() != self.regs.get_of(),
            0xD => self.regs.get_sf() == self.regs.get_of(),
            0xE => self.regs.get_zf() || (self.regs.get_sf() != self.regs.get_of()),
            0xF => !self.regs.get_zf() && (self.regs.get_sf() == self.regs.get_of()),
            _ => false,
        }
    }
}
