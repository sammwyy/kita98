use crate::bus::Bus;
use crate::cpu::regs::{Reg16, SegReg};
use super::{Interpreter, StepResult};
use anyhow::Result;

impl Interpreter {
    pub fn dispatch_flow(&mut self, opcode: u8, bus: &mut Bus, ip_before: u32) -> Result<StepResult> {
        match opcode {
            // Jumps (short)
            0x70..=0x7F => {
                let rel = self.fetch_i8(bus) as i32;
                let cond = match opcode & 0x0F {
                    0x0 => self.regs.get_of(),         // JO
                    0x1 => !self.regs.get_of(),        // JNO
                    0x2 => self.regs.get_cf(),         // JB/JNAE
                    0x3 => !self.regs.get_cf(),        // JNB/JAE
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
                    _ => unreachable!(),
                };
                if cond {
                    self.regs.ip = self.regs.ip.wrapping_add(rel as u32) & 0xFFFF;
                }
                Ok(StepResult::Continue)
            }

            // LOOP / LOOPE / LOOPNE / JCXZ
            0xE0..=0xE3 => {
                let rel = self.fetch_i8(bus) as i32;
                let cond = match opcode {
                    0xE0 => {
                        let cx = self.regs.get16(Reg16::CX).wrapping_sub(1);
                        self.regs.set16(Reg16::CX, cx);
                        cx != 0 && !self.regs.get_zf()
                    }
                    0xE1 => {
                        let cx = self.regs.get16(Reg16::CX).wrapping_sub(1);
                        self.regs.set16(Reg16::CX, cx);
                        cx != 0 && self.regs.get_zf()
                    }
                    0xE2 => {
                        let cx = self.regs.get16(Reg16::CX).wrapping_sub(1);
                        self.regs.set16(Reg16::CX, cx);
                        cx != 0
                    }
                    0xE3 => self.regs.get16(Reg16::CX) == 0,
                    _ => unreachable!(),
                };
                if cond {
                    self.regs.ip = self.regs.ip.wrapping_add(rel as u32) & 0xFFFF;
                }
                Ok(StepResult::Continue)
            }

            // Unconditional Jumps
            0xEB => {
                // JMP rel8
                let rel = self.fetch_i8(bus) as i32;
                self.regs.ip = self.regs.ip.wrapping_add(rel as u32) & 0xFFFF;
                Ok(StepResult::Continue)
            }
            0xE9 => {
                // JMP rel16
                let rel = self.fetch_i16(bus) as i32;
                self.regs.ip = self.regs.ip.wrapping_add(rel as u32) & 0xFFFF;
                Ok(StepResult::Continue)
            }
            0xEA => {
                // JMP far ptr
                let off = self.fetch_u16(bus);
                let seg = self.fetch_u16(bus);
                self.regs.ip = off as u32;
                self.regs.set_seg(SegReg::CS, seg);
                Ok(StepResult::Continue)
            }

            // CALL
            0xE8 => {
                // CALL rel16
                let rel = self.fetch_i16(bus) as i32;
                let next_ip = self.regs.ip as u16;
                self.push16(bus, next_ip);
                self.regs.ip = self.regs.ip.wrapping_add(rel as u32) & 0xFFFF;
                Ok(StepResult::Continue)
            }
            0x9A => {
                // CALL far ptr
                let off = self.fetch_u16(bus);
                let seg = self.fetch_u16(bus);
                let cur_cs = self.regs.get_seg(SegReg::CS);
                let cur_ip = self.regs.ip as u16;
                self.push16(bus, cur_cs);
                self.push16(bus, cur_ip);
                self.regs.ip = off as u32;
                self.regs.set_seg(SegReg::CS, seg);
                Ok(StepResult::Continue)
            }

            // RET
            0xC2 => {
                // RET imm16 (near)
                let n = self.fetch_u16(bus);
                let ip = self.pop16(bus);
                self.regs.ip = ip as u32;
                let sp = self.regs.get16(Reg16::SP).wrapping_add(n);
                self.regs.set16(Reg16::SP, sp);
                Ok(StepResult::Continue)
            }
            0xC3 => {
                // RET (near)
                let ip = self.pop16(bus);
                self.regs.ip = ip as u32;
                Ok(StepResult::Continue)
            }
            0xCA => {
                // RET imm16 (far)
                let n = self.fetch_u16(bus);
                let ip = self.pop16(bus);
                let cs = self.pop16(bus);
                self.regs.ip = ip as u32;
                self.regs.set_seg(SegReg::CS, cs);
                let sp = self.regs.get16(Reg16::SP).wrapping_add(n);
                self.regs.set16(Reg16::SP, sp);
                Ok(StepResult::Continue)
            }
            0xCB => {
                // RET (far)
                let ip = self.pop16(bus);
                let cs = self.pop16(bus);
                self.regs.ip = ip as u32;
                self.regs.set_seg(SegReg::CS, cs);
                Ok(StepResult::Continue)
            }
            0xCF => {
                // IRET
                let new_ip = self.pop16(bus);
                let new_cs = self.pop16(bus);
                let new_flags = self.pop16(bus);
                self.regs.ip = new_ip as u32;
                self.regs.set_seg(SegReg::CS, new_cs);
                self.regs.flags = (self.regs.flags & !0xFFFF) | (new_flags as u32);
                Ok(StepResult::Continue)
            }

            // Group FF
            0xFF => {
                let mb = self.fetch_u8(bus);
                let modrm = crate::cpu::interp::modrm::ModRm::decode(mb);
                match modrm.reg {
                    0 => { // INC r/m16
                        let v = self.read_modrm_u16(&modrm, bus);
                        let r = v.wrapping_add(1);
                        let old_cf = self.regs.get_cf();
                        self.update_add16(v, 1, r);
                        self.regs.set_cf(old_cf);
                        self.write_modrm_u16(&modrm, bus, r);
                    }
                    1 => { // DEC r/m16
                        let v = self.read_modrm_u16(&modrm, bus);
                        let r = v.wrapping_sub(1);
                        let old_cf = self.regs.get_cf();
                        self.update_sub16(v, 1, r);
                        self.regs.set_cf(old_cf);
                        self.write_modrm_u16(&modrm, bus, r);
                    }
                    2 => { // CALL near
                        let target = self.read_modrm_u16(&modrm, bus);
                        let next_ip = self.regs.ip as u16;
                        self.push16(bus, next_ip);
                        self.regs.ip = target as u32;
                    }
                    3 => { // CALL far
                        let ea = self.ea_from_modrm(&modrm, bus);
                        let off = bus.mem.read_u16(ea);
                        let seg = bus.mem.read_u16(ea + 2);
                        let cur_cs = self.regs.get_seg(SegReg::CS);
                        let cur_ip = self.regs.ip as u16;
                        self.push16(bus, cur_cs);
                        self.push16(bus, cur_ip);
                        self.regs.ip = off as u32;
                        self.regs.set_seg(SegReg::CS, seg);
                    }
                    4 => { // JMP near
                        let target = self.read_modrm_u16(&modrm, bus);
                        self.regs.ip = target as u32;
                    }
                    5 => { // JMP far
                        let ea = self.ea_from_modrm(&modrm, bus);
                        let off = bus.mem.read_u16(ea);
                        let seg = bus.mem.read_u16(ea + 2);
                        self.regs.ip = off as u32;
                        self.regs.set_seg(SegReg::CS, seg);
                    }
                    6 => { // PUSH r/m16
                        let val = self.read_modrm_u16(&modrm, bus);
                        self.push16(bus, val);
                    }
                    _ => return Ok(StepResult::Unimplemented(0xFF)),
                }
                Ok(StepResult::Continue)
            }

            _ => self.dispatch_misc(opcode, bus, ip_before),
        }
    }
}
