use crate::bus::Bus;
use crate::cpu::regs::{Reg16, Reg8, SegReg};
use super::{Interpreter, StepResult};
use super::modrm::ModRm;
use anyhow::Result;

impl Interpreter {
    pub fn dispatch_misc(&mut self, opcode: u8, bus: &mut Bus, _ip_before: u32) -> Result<StepResult> {
        match opcode {
            // Extended
            0x0F => self.dispatch_0f(bus),

            // NOP / HLT
            0x90 => Ok(StepResult::Continue),
            0xF4 => {
                self.halted = true;
                log::info!("HLT - execution stopped");
                Ok(StepResult::Halt)
            }

            // INT
            0xCD => {
                let num = self.fetch_u8(bus);
                Ok(StepResult::Interrupt(num))
            }
            0xCE => Ok(StepResult::Interrupt(4)), // INTO

            // SALC
            0xD6 => {
                let val = if self.regs.get_cf() { 0xFF } else { 0x00 };
                self.regs.set8(Reg8::AL, val);
                Ok(StepResult::Continue)
            }

            // Flags
            0xFA => { self.regs.set_flag(crate::cpu::regs::flags::IF, false); Ok(StepResult::Continue) }
            0xFB => { self.regs.set_flag(crate::cpu::regs::flags::IF, true); Ok(StepResult::Continue) }
            0xFC => { self.regs.set_df(false); Ok(StepResult::Continue) }
            0xFD => { self.regs.set_df(true); Ok(StepResult::Continue) }
            0xF5 => { let c = !self.regs.get_cf(); self.regs.set_cf(c); Ok(StepResult::Continue) }
            0xF8 => { self.regs.set_cf(false); Ok(StepResult::Continue) }
            0xF9 => { self.regs.set_cf(true); Ok(StepResult::Continue) }

            // BCD / AAM / AAD
            0x27 | 0x2F | 0x37 | 0x3F => Ok(StepResult::Continue),
            0xD4 | 0xD5 => { self.fetch_u8(bus); Ok(StepResult::Continue) }

            // IN / OUT
            0xE4 => {
                let port = self.fetch_u8(bus) as u16;
                let val = bus.io_read_u8(port);
                self.regs.set8(Reg8::AL, val);
                Ok(StepResult::Continue)
            }
            0xE5 => {
                let port = self.fetch_u8(bus) as u16;
                let val_lo = bus.io_read_u8(port);
                let val_hi = bus.io_read_u8(port.wrapping_add(1));
                self.regs.set16(Reg16::AX, ((val_hi as u16) << 8) | val_lo as u16);
                Ok(StepResult::Continue)
            }
            0xE6 => {
                let port = self.fetch_u8(bus) as u16;
                let val = self.regs.get8(Reg8::AL);
                bus.io_write_u8(port, val);
                Ok(StepResult::Continue)
            }
            0xE7 => {
                let port = self.fetch_u8(bus) as u16;
                let val = self.regs.get16(Reg16::AX);
                bus.io_write_u8(port, val as u8);
                bus.io_write_u8(port.wrapping_add(1), (val >> 8) as u8);
                Ok(StepResult::Continue)
            }
            0xEC => {
                let port = self.regs.get16(Reg16::DX);
                let val = bus.io_read_u8(port);
                self.regs.set8(Reg8::AL, val);
                Ok(StepResult::Continue)
            }
            0xED => {
                let port = self.regs.get16(Reg16::DX);
                let val_lo = bus.io_read_u8(port);
                let val_hi = bus.io_read_u8(port.wrapping_add(1));
                self.regs.set16(Reg16::AX, ((val_hi as u16) << 8) | val_lo as u16);
                Ok(StepResult::Continue)
            }
            0xEE => {
                let port = self.regs.get16(Reg16::DX);
                let val = self.regs.get8(Reg8::AL);
                bus.io_write_u8(port, val);
                Ok(StepResult::Continue)
            }
            0xEF => {
                let port = self.regs.get16(Reg16::DX);
                let val = self.regs.get16(Reg16::AX);
                bus.io_write_u8(port, val as u8);
                bus.io_write_u8(port.wrapping_add(1), (val >> 8) as u8);
                Ok(StepResult::Continue)
            }

            // MOV seg
            0x8C => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let val = self.regs.get_seg(Self::seg_from_field(modrm.reg));
                self.write_modrm_u16(&modrm, bus, val);
                Ok(StepResult::Continue)
            }
            0x8E => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let val = self.read_modrm_u16(&modrm, bus);
                self.regs.set_seg(Self::seg_from_field(modrm.reg), val);
                Ok(StepResult::Continue)
            }

            // FPU stubs
            0xD8..=0xDF => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                if modrm.mode != 3 { self.decode_ea(&modrm, bus); }
                Ok(StepResult::Continue)
            }

            _ => Ok(StepResult::Unimplemented(opcode)),
        }
    }
}
