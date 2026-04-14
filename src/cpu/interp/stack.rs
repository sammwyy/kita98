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
}
