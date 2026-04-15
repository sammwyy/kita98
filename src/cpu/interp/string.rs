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

    pub fn cmpsb(&mut self, bus: &mut Bus) {
        let si = self.regs.get16(Reg16::SI);
        let di = self.regs.get16(Reg16::DI);
        let seg = self.prefix.seg_override.unwrap_or(SegReg::DS);
        let a = bus.mem.seg_read_u8(self.regs.get_seg(seg), si as u32);
        let b = bus.mem.seg_read_u8(self.regs.get_seg(SegReg::ES), di as u32);
        let r = a.wrapping_sub(b);
        self.update_sub8(a, b, r);
        self.str_si_advance(1);
        self.str_di_advance(1);
    }

    pub fn cmpsw(&mut self, bus: &mut Bus) {
        let si = self.regs.get16(Reg16::SI);
        let di = self.regs.get16(Reg16::DI);
        let seg = self.prefix.seg_override.unwrap_or(SegReg::DS);
        let a = bus.mem.seg_read_u16(self.regs.get_seg(seg), si as u32);
        let b = bus.mem.seg_read_u16(self.regs.get_seg(SegReg::ES), di as u32);
        let r = a.wrapping_sub(b);
        self.update_sub16(a, b, r);
        self.str_si_advance(2);
        self.str_di_advance(2);
    }

    pub fn dispatch_string(&mut self, opcode: u8, bus: &mut Bus, ip_before: u32) -> anyhow::Result<crate::cpu::interp::StepResult> {
        use crate::cpu::interp::StepResult;
        match opcode {
            // String Ops
            0xA4 => { self.movsb(bus); Ok(StepResult::Continue) }
            0xA5 => { self.movsw(bus); Ok(StepResult::Continue) }
            0xAA => { self.stosb(bus); Ok(StepResult::Continue) }
            0xAB => { self.stosw(bus); Ok(StepResult::Continue) }
            0xAC => { self.lodsb(bus); Ok(StepResult::Continue) }
            0xAD => { self.lodsw(bus); Ok(StepResult::Continue) }
            0xA6 => { self.cmpsb(bus); Ok(StepResult::Continue) }
            0xA7 => { self.cmpsw(bus); Ok(StepResult::Continue) }
            0xAE => { self.scasb(bus); Ok(StepResult::Continue) }
            0xAF => { self.scasw(bus); Ok(StepResult::Continue) }

            // REPs
            0xF3 => {
                // Skip and handle any prefix bytes
                let mut next = self.fetch_u8(bus);
                loop {
                    match next {
                        0x26 => self.prefix.seg_override = Some(SegReg::ES),
                        0x2E => self.prefix.seg_override = Some(SegReg::CS),
                        0x36 => self.prefix.seg_override = Some(SegReg::SS),
                        0x3E => self.prefix.seg_override = Some(SegReg::DS),
                        0x64 => self.prefix.seg_override = Some(SegReg::FS),
                        0x65 => self.prefix.seg_override = Some(SegReg::GS),
                        0x66 => self.prefix.operand_32 = !self.prefix.operand_32,
                        0x67 => self.prefix.address_32 = !self.prefix.address_32,
                        0xF0 => {} // LOCK
                        _ => break,
                    }
                    next = self.fetch_u8(bus);
                }
                match next {
                    0xA4 | 0xA5 | 0xAA | 0xAB | 0xAC | 0xAD | 0xA6 | 0xA7 | 0xAE | 0xAF => {
                        loop {
                            let cx = self.regs.get16(Reg16::CX);
                            if cx == 0 { break; }
                            self.regs.set16(Reg16::CX, cx.wrapping_sub(1));
                            match next {
                                0xA4 => self.movsb(bus),
                                0xA5 => self.movsw(bus),
                                0xAA => self.stosb(bus),
                                0xAB => self.stosw(bus),
                                0xAC => self.lodsb(bus),
                                0xAD => self.lodsw(bus),
                                0xA6 => { self.cmpsb(bus); if !self.regs.get_zf() { break; } }
                                0xA7 => { self.cmpsw(bus); if !self.regs.get_zf() { break; } }
                                0xAE => { self.scasb(bus); if !self.regs.get_zf() { break; } }
                                0xAF => { self.scasw(bus); if !self.regs.get_zf() { break; } }
                                _ => unreachable!(),
                            }
                        }
                        Ok(StepResult::Continue)
                    }
                    _ => self.dispatch(next, bus, ip_before),
                }
            }
            0xF2 => {
                let mut next = self.fetch_u8(bus);
                loop {
                    match next {
                        0x26 => self.prefix.seg_override = Some(SegReg::ES),
                        0x2E => self.prefix.seg_override = Some(SegReg::CS),
                        0x36 => self.prefix.seg_override = Some(SegReg::SS),
                        0x3E => self.prefix.seg_override = Some(SegReg::DS),
                        0x64 => self.prefix.seg_override = Some(SegReg::FS),
                        0x65 => self.prefix.seg_override = Some(SegReg::GS),
                        0x66 => self.prefix.operand_32 = !self.prefix.operand_32,
                        0x67 => self.prefix.address_32 = !self.prefix.address_32,
                        _ => break,
                    }
                    next = self.fetch_u8(bus);
                }
                match next {
                    0xA6 | 0xA7 | 0xAE | 0xAF => {
                        loop {
                            let cx = self.regs.get16(Reg16::CX);
                            if cx == 0 { break; }
                            self.regs.set16(Reg16::CX, cx.wrapping_sub(1));
                            match next {
                                0xA6 => { self.cmpsb(bus); if self.regs.get_zf() { break; } }
                                0xA7 => { self.cmpsw(bus); if self.regs.get_zf() { break; } }
                                0xAE => { self.scasb(bus); if self.regs.get_zf() { break; } }
                                0xAF => { self.scasw(bus); if self.regs.get_zf() { break; } }
                                _ => unreachable!(),
                            }
                        }
                        Ok(StepResult::Continue)
                    }
                    _ => self.dispatch(next, bus, ip_before),
                }
            }

            _ => self.dispatch_transfer(opcode, bus, ip_before),
        }
    }
}
