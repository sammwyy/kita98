use crate::bus::Bus;
use crate::cpu::regs::{Reg16, Reg8, SegReg};
use super::Interpreter;

impl Interpreter {
    pub fn str_di_advance(&mut self, delta: u16) {
        if self.regs.get_df() {
            let di = self.regs.get16(Reg16::DI).wrapping_sub(delta);
            self.regs.set16(Reg16::DI, di);
        } else {
            let di = self.regs.get16(Reg16::DI).wrapping_add(delta);
            self.regs.set16(Reg16::DI, di);
        }
    }

    pub fn str_si_advance(&mut self, delta: u16) {
        if self.regs.get_df() {
            let si = self.regs.get16(Reg16::SI).wrapping_sub(delta);
            self.regs.set16(Reg16::SI, si);
        } else {
            let si = self.regs.get16(Reg16::SI).wrapping_add(delta);
            self.regs.set16(Reg16::SI, si);
        }
    }

    pub fn movsb(&mut self, bus: &mut Bus) {
        let si = self.regs.get16(Reg16::SI);
        let di = self.regs.get16(Reg16::DI);
        let seg = self.prefix.seg_override.unwrap_or(SegReg::DS);
        let v = bus.mem.seg_read_u8(self.regs.get_seg(seg), si as u32);
        bus.mem.seg_write_u8(self.regs.get_seg(SegReg::ES), di as u32, v);
        self.str_si_advance(1);
        self.str_di_advance(1);
    }

    pub fn movsw(&mut self, bus: &mut Bus) {
        let si = self.regs.get16(Reg16::SI);
        let di = self.regs.get16(Reg16::DI);
        let seg = self.prefix.seg_override.unwrap_or(SegReg::DS);
        let v = bus.mem.seg_read_u16(self.regs.get_seg(seg), si as u32);
        bus.mem.seg_write_u16(self.regs.get_seg(SegReg::ES), di as u32, v);
        self.str_si_advance(2);
        self.str_di_advance(2);
    }

    pub fn stosb(&mut self, bus: &mut Bus) {
        let di = self.regs.get16(Reg16::DI);
        let v = self.regs.get8(Reg8::AL);
        bus.mem.seg_write_u8(self.regs.get_seg(SegReg::ES), di as u32, v);
        self.str_di_advance(1);
    }

    pub fn stosw(&mut self, bus: &mut Bus) {
        let di = self.regs.get16(Reg16::DI);
        let v = self.regs.get16(Reg16::AX);
        bus.mem.seg_write_u16(self.regs.get_seg(SegReg::ES), di as u32, v);
        self.str_di_advance(2);
    }

    pub fn lodsb(&mut self, bus: &mut Bus) {
        let si = self.regs.get16(Reg16::SI);
        let seg = self.prefix.seg_override.unwrap_or(SegReg::DS);
        let v = bus.mem.seg_read_u8(self.regs.get_seg(seg), si as u32);
        self.regs.set8(Reg8::AL, v);
        self.str_si_advance(1);
    }

    pub fn lodsw(&mut self, bus: &mut Bus) {
        let si = self.regs.get16(Reg16::SI);
        let seg = self.prefix.seg_override.unwrap_or(SegReg::DS);
        let v = bus.mem.seg_read_u16(self.regs.get_seg(seg), si as u32);
        self.regs.set16(Reg16::AX, v);
        self.str_si_advance(2);
    }

    pub fn scasb(&mut self, bus: &mut Bus) {
        let di = self.regs.get16(Reg16::DI);
        let v = bus.mem.seg_read_u8(self.regs.get_seg(SegReg::ES), di as u32);
        let al = self.regs.get8(Reg8::AL);
        let r = al.wrapping_sub(v);
        self.update_sub8(al, v, r);
        self.str_di_advance(1);
    }

    pub fn scasw(&mut self, bus: &mut Bus) {
        let di = self.regs.get16(Reg16::DI);
        let v = bus.mem.seg_read_u16(self.regs.get_seg(SegReg::ES), di as u32);
        let ax = self.regs.get16(Reg16::AX);
        let r = ax.wrapping_sub(v);
        self.update_sub16(ax, v, r);
        self.str_di_advance(2);
    }
}
