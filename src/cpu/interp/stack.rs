use crate::bus::Bus;
use crate::cpu::regs::{Reg16, SegReg};
use super::Interpreter;

impl Interpreter {
    pub fn push16(&mut self, bus: &mut Bus, val: u16) {
        let sp = self.regs.get16(Reg16::SP).wrapping_sub(2);
        self.regs.set16(Reg16::SP, sp);
        let ss = self.regs.get_seg(SegReg::SS);
        bus.mem.seg_write_u16(ss, sp as u32, val);
    }

    pub fn pop16(&mut self, bus: &mut Bus) -> u16 {
        let sp = self.regs.get16(Reg16::SP);
        let ss = self.regs.get_seg(SegReg::SS);
        let val = bus.mem.seg_read_u16(ss, sp as u32);
        self.regs.set16(Reg16::SP, sp.wrapping_add(2));
        val
    }

    #[allow(dead_code)]
    pub fn push32(&mut self, bus: &mut Bus, val: u32) {
        let sp = self.regs.get32(Reg16::SP).wrapping_sub(4);
        self.regs.set32(Reg16::SP, sp);
        let ss = self.regs.get_seg(SegReg::SS);
        bus.mem.seg_write_u32(ss, sp, val);
    }

    #[allow(dead_code)]
    pub fn pop32(&mut self, bus: &mut Bus) -> u32 {
        let sp = self.regs.get32(Reg16::SP);
        let ss = self.regs.get_seg(SegReg::SS);
        let val = bus.mem.seg_read_u32(ss, sp);
        self.regs.set32(Reg16::SP, sp.wrapping_add(4));
        val
    }

    pub fn dispatch_stack(&mut self, opcode: u8, bus: &mut Bus, ip_before: u32) -> anyhow::Result<crate::cpu::interp::StepResult> {
        use crate::cpu::regs::{Reg16, Regs, Reg8, SegReg};
        use crate::cpu::interp::StepResult;

        match opcode {
            // PUSH r16
            0x50..=0x57 => {
                let reg = Self::reg16_from_field(opcode & 7);
                let val = self.regs.get16(reg);
                self.push16(bus, val);
                Ok(StepResult::Continue)
            }
            // POP r16
            0x58..=0x5F => {
                let reg = Self::reg16_from_field(opcode & 7);
                let val = self.pop16(bus);
                self.regs.set16(reg, val);
                Ok(StepResult::Continue)
            }
            // PUSH segment
            0x06 => { let v = self.regs.get_seg(SegReg::ES); self.push16(bus, v); Ok(StepResult::Continue) }
            0x0E => { let v = self.regs.get_seg(SegReg::CS); self.push16(bus, v); Ok(StepResult::Continue) }
            0x16 => { let v = self.regs.get_seg(SegReg::SS); self.push16(bus, v); Ok(StepResult::Continue) }
            0x1E => { let v = self.regs.get_seg(SegReg::DS); self.push16(bus, v); Ok(StepResult::Continue) }
            // POP segment
            0x07 => { let v = self.pop16(bus); self.regs.set_seg(SegReg::ES, v); Ok(StepResult::Continue) }
            0x17 => { let v = self.pop16(bus); self.regs.set_seg(SegReg::SS, v); Ok(StepResult::Continue) }
            0x1F => { let v = self.pop16(bus); self.regs.set_seg(SegReg::DS, v); Ok(StepResult::Continue) }
            
            // PUSHA / POPA
            0x60 => {
                let sp = self.regs.get16(Reg16::SP);
                self.push16(bus, self.regs.get16(Reg16::AX));
                self.push16(bus, self.regs.get16(Reg16::CX));
                self.push16(bus, self.regs.get16(Reg16::DX));
                self.push16(bus, self.regs.get16(Reg16::BX));
                self.push16(bus, sp);
                self.push16(bus, self.regs.get16(Reg16::BP));
                self.push16(bus, self.regs.get16(Reg16::SI));
                self.push16(bus, self.regs.get16(Reg16::DI));
                Ok(StepResult::Continue)
            }
            0x61 => {
                let di = self.pop16(bus);
                let si = self.pop16(bus);
                let bp = self.pop16(bus);
                self.pop16(bus); // throw away SP
                let bx = self.pop16(bus);
                let dx = self.pop16(bus);
                let cx = self.pop16(bus);
                let ax = self.pop16(bus);
                self.regs.set16(Reg16::DI, di);
                self.regs.set16(Reg16::SI, si);
                self.regs.set16(Reg16::BP, bp);
                self.regs.set16(Reg16::BX, bx);
                self.regs.set16(Reg16::DX, dx);
                self.regs.set16(Reg16::CX, cx);
                self.regs.set16(Reg16::AX, ax);
                Ok(StepResult::Continue)
            }

            // PUSHF / POPF
            0x9C => {
                let flags = self.regs.flags as u16;
                self.push16(bus, flags);
                Ok(StepResult::Continue)
            }
            0x9D => {
                let flags = self.pop16(bus);
                self.regs.flags = (self.regs.flags & 0xFFFF0000) | (flags as u32);
                Ok(StepResult::Continue)
            }

            _ => self.dispatch_string(opcode, bus, ip_before),
        }
    }
}
