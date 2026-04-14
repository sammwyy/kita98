use anyhow::Result;
use std::collections::HashSet;

use crate::bus::Bus;
use crate::cpu::regs::{Reg16, Reg8, Regs, SegReg};
use crate::memory::Memory;

pub mod alu;
pub mod extended;
pub mod modrm;
pub mod stack;
pub mod string;

/// Return value from `execute_one`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepResult {
    /// Normal: continue execution.
    Continue,
    /// HLT executed – stop the run loop.
    Halt,
    /// INT instruction – caller handles it.
    Interrupt(u8),
    /// Unimplemented opcode encountered.
    Unimplemented(u8),
}

#[derive(Debug, Clone, Default)]
pub struct PrefixState {
    pub operand_32: bool,
    pub address_32: bool,
    pub seg_override: Option<SegReg>,
}

pub struct Interpreter {
    pub regs: Regs,
    pub halted: bool,
    pub trace: bool,
    pub dump_every: u64,
    pub instructions_executed: u64,
    pub prefix: PrefixState,
    pub breakpoints: HashSet<u32>,
    pub valid_ranges: Vec<std::ops::Range<u32>>,
}

impl Interpreter {
    pub fn new() -> Self {
        Self {
            regs: Regs::new(),
            halted: false,
            trace: true,
            dump_every: 100,
            instructions_executed: 0,
            prefix: PrefixState::default(),
            breakpoints: HashSet::new(),
            valid_ranges: Vec::new(),
        }
    }

    // IP fetch helpers

    pub fn fetch_u8(&mut self, bus: &Bus) -> u8 {
        let v = bus
            .mem
            .seg_read_u8(self.regs.get_seg(SegReg::CS), self.regs.ip);
        self.regs.ip = self.regs.ip.wrapping_add(1);
        v
    }

    pub fn fetch_u16(&mut self, bus: &Bus) -> u16 {
        let lo = self.fetch_u8(bus) as u16;
        let hi = self.fetch_u8(bus) as u16;
        lo | (hi << 8)
    }

    pub fn fetch_u32(&mut self, bus: &Bus) -> u32 {
        let lo = self.fetch_u16(bus) as u32;
        let hi = self.fetch_u16(bus) as u32;
        lo | (hi << 16)
    }

    pub fn fetch_i8(&mut self, bus: &Bus) -> i8 {
        self.fetch_u8(bus) as i8
    }

    pub fn fetch_i16(&mut self, bus: &Bus) -> i16 {
        self.fetch_u16(bus) as i16
    }

    pub fn fetch_i32(&mut self, bus: &Bus) -> i32 {
        self.fetch_u32(bus) as i32
    }

    /// Fetch, decode and execute one instruction.
    pub fn execute_one(&mut self, bus: &mut Bus) -> Result<StepResult> {
        if self.halted {
            return Ok(StepResult::Halt);
        }

        let cs = self.regs.get_seg(SegReg::CS);
        let ip_before = self.regs.ip;
        let phys_ip_start = Memory::phys(cs, ip_before);

        if self.breakpoints.contains(&phys_ip_start) {
            log::info!("Breakpoint hit at {:04X}:{:08X}", cs, ip_before);
        }

        self.prefix = PrefixState::default();

        let mut opcode = self.fetch_u8(bus);
        loop {
            match opcode {
                0x26 => self.prefix.seg_override = Some(SegReg::ES),
                0x2E => self.prefix.seg_override = Some(SegReg::CS),
                0x36 => self.prefix.seg_override = Some(SegReg::SS),
                0x3E => self.prefix.seg_override = Some(SegReg::DS),
                0x64 => self.prefix.seg_override = Some(SegReg::FS),
                0x65 => self.prefix.seg_override = Some(SegReg::GS),
                0x66 => self.prefix.operand_32 = !self.prefix.operand_32,
                0x67 => self.prefix.address_32 = !self.prefix.address_32,
                0xF0 => {}            // LOCK
                0xF2 | 0xF3 => break, // Handled in dispatch
                _ => break,
            }
            opcode = self.fetch_u8(bus);
        }

        let result = self.dispatch(opcode, bus, ip_before)?;

        if self.trace {
            let ip_after = self.regs.ip;
            let len = ip_after.wrapping_sub(ip_before) as usize;
            let raw: Vec<String> = bus
                .mem
                .read_bytes(phys_ip_start, len)
                .iter()
                .map(|b| format!("{:02X}", b))
                .collect();
            log::info!(
                "[{:05X}] {:04X}:{:08X}  {:16}  {:?}",
                phys_ip_start,
                cs,
                ip_before,
                raw.join(" "),
                result
            );
        }

        self.instructions_executed += 1;

        if self.dump_every > 0 && self.instructions_executed % self.dump_every == 0 {
            log::info!("REGS: {}", self.regs.dump());
        }

        Ok(result)
    }

    pub fn dispatch(&mut self, opcode: u8, bus: &mut Bus, ip_before: u32) -> Result<StepResult> {
        use crate::cpu::interp::modrm::ModRm;
        match opcode {
            // Extended / NOP / HLT / INT / IRET
            0x0F => self.dispatch_0f(bus),
            0x90 => Ok(StepResult::Continue), // NOP
            0xF4 => {
                self.halted = true;
                log::info!("HLT - execution stopped");
                Ok(StepResult::Halt)
            }
            0xCD => {
                let num = self.fetch_u8(bus);
                Ok(StepResult::Interrupt(num))
            }
            0xCE => Ok(StepResult::Interrupt(4)), // INTO
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

            // Flag instructions
            0xFA => {
                self.regs.set_flag(crate::cpu::regs::flags::IF, false);
                Ok(StepResult::Continue)
            }
            0xFB => {
                self.regs.set_flag(crate::cpu::regs::flags::IF, true);
                Ok(StepResult::Continue)
            }
            0xFC => {
                self.regs.set_df(false);
                Ok(StepResult::Continue)
            }
            0xFD => {
                self.regs.set_df(true);
                Ok(StepResult::Continue)
            }
            0xF8 => {
                self.regs.set_cf(false);
                Ok(StepResult::Continue)
            }
            0xF9 => {
                self.regs.set_cf(true);
                Ok(StepResult::Continue)
            }
            0xF5 => {
                let c = !self.regs.get_cf();
                self.regs.set_cf(c);
                Ok(StepResult::Continue)
            }

            // ADD AL/AX, imm
            0x04 => {
                let imm = self.fetch_u8(bus);
                let al = self.regs.get8(Reg8::AL);
                let r = al.wrapping_add(imm);
                self.update_add8(al, imm, r);
                self.regs.set8(Reg8::AL, r);
                Ok(StepResult::Continue)
            }
            0x05 => {
                let imm = self.fetch_u16(bus);
                let ax = self.regs.get16(Reg16::AX);
                let r = ax.wrapping_add(imm);
                self.update_add16(ax, imm, r);
                self.regs.set16(Reg16::AX, r);
                Ok(StepResult::Continue)
            }
            // OR AL/AX, imm
            0x0C => {
                let imm = self.fetch_u8(bus);
                let r = self.regs.get8(Reg8::AL) | imm;
                self.regs.update_flags_u8(r);
                self.regs.set_cf(false);
                self.regs.set_of(false);
                self.regs.set8(Reg8::AL, r);
                Ok(StepResult::Continue)
            }
            0x0D => {
                let imm = self.fetch_u16(bus);
                let r = self.regs.get16(Reg16::AX) | imm;
                self.regs.update_flags_u16(r);
                self.regs.set_cf(false);
                self.regs.set_of(false);
                self.regs.set16(Reg16::AX, r);
                Ok(StepResult::Continue)
            }
            // ADC AL/AX, imm
            0x14 => {
                let imm = self.fetch_u8(bus);
                let cf = self.regs.get_cf() as u8;
                let al = self.regs.get8(Reg8::AL);
                let r = al.wrapping_add(imm).wrapping_add(cf);
                self.update_add8(al, imm.wrapping_add(cf), r);
                self.regs.set8(Reg8::AL, r);
                Ok(StepResult::Continue)
            }
            0x15 => {
                let imm = self.fetch_u16(bus);
                let cf = self.regs.get_cf() as u16;
                let ax = self.regs.get16(Reg16::AX);
                let r = ax.wrapping_add(imm).wrapping_add(cf);
                self.update_add16(ax, imm.wrapping_add(cf), r);
                self.regs.set16(Reg16::AX, r);
                Ok(StepResult::Continue)
            }
            // SBB AL/AX, imm
            0x1C => {
                let imm = self.fetch_u8(bus);
                let cf = self.regs.get_cf() as u8;
                let al = self.regs.get8(Reg8::AL);
                let r = al.wrapping_sub(imm).wrapping_sub(cf);
                self.update_sub8(al, imm.wrapping_add(cf), r);
                self.regs.set8(Reg8::AL, r);
                Ok(StepResult::Continue)
            }
            0x1D => {
                let imm = self.fetch_u16(bus);
                let cf = self.regs.get_cf() as u16;
                let ax = self.regs.get16(Reg16::AX);
                let r = ax.wrapping_sub(imm).wrapping_sub(cf);
                self.update_sub16(ax, imm.wrapping_add(cf), r);
                self.regs.set16(Reg16::AX, r);
                Ok(StepResult::Continue)
            }
            // AND AL/AX, imm
            0x24 => {
                let imm = self.fetch_u8(bus);
                let r = self.regs.get8(Reg8::AL) & imm;
                self.regs.update_flags_u8(r);
                self.regs.set_cf(false);
                self.regs.set_of(false);
                self.regs.set8(Reg8::AL, r);
                Ok(StepResult::Continue)
            }
            0x25 => {
                let imm = self.fetch_u16(bus);
                let r = self.regs.get16(Reg16::AX) & imm;
                self.regs.update_flags_u16(r);
                self.regs.set_cf(false);
                self.regs.set_of(false);
                self.regs.set16(Reg16::AX, r);
                Ok(StepResult::Continue)
            }
            // SUB AL/AX, imm
            0x2C => {
                let imm = self.fetch_u8(bus);
                let al = self.regs.get8(Reg8::AL);
                let r = al.wrapping_sub(imm);
                self.update_sub8(al, imm, r);
                self.regs.set8(Reg8::AL, r);
                Ok(StepResult::Continue)
            }
            0x2D => {
                let imm = self.fetch_u16(bus);
                let ax = self.regs.get16(Reg16::AX);
                let r = ax.wrapping_sub(imm);
                self.update_sub16(ax, imm, r);
                self.regs.set16(Reg16::AX, r);
                Ok(StepResult::Continue)
            }
            // XOR AL/AX, imm
            0x34 => {
                let imm = self.fetch_u8(bus);
                let r = self.regs.get8(Reg8::AL) ^ imm;
                self.regs.update_flags_u8(r);
                self.regs.set_cf(false);
                self.regs.set_of(false);
                self.regs.set8(Reg8::AL, r);
                Ok(StepResult::Continue)
            }
            0x35 => {
                let imm = self.fetch_u16(bus);
                let r = self.regs.get16(Reg16::AX) ^ imm;
                self.regs.update_flags_u16(r);
                self.regs.set_cf(false);
                self.regs.set_of(false);
                self.regs.set16(Reg16::AX, r);
                Ok(StepResult::Continue)
            }
            // CMP AL/AX, imm
            0x3C => {
                let imm = self.fetch_u8(bus);
                let al = self.regs.get8(Reg8::AL);
                let r = al.wrapping_sub(imm);
                self.update_sub8(al, imm, r);
                Ok(StepResult::Continue)
            }
            0x3D => {
                let imm = self.fetch_u16(bus);
                let ax = self.regs.get16(Reg16::AX);
                let r = ax.wrapping_sub(imm);
                self.update_sub16(ax, imm, r);
                Ok(StepResult::Continue)
            }

            // ALU r/m, r
            // ADD
            0x00 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let s = self.regs.get8(Self::reg8_from_field(modrm.reg));
                let d = self.read_modrm_u8(&modrm, bus);
                let r = d.wrapping_add(s);
                self.update_add8(d, s, r);
                self.write_modrm_u8(&modrm, bus, r);
                Ok(StepResult::Continue)
            }
            0x01 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let s = self.regs.get16(Self::reg16_from_field(modrm.reg));
                let d = self.read_modrm_u16(&modrm, bus);
                let r = d.wrapping_add(s);
                self.update_add16(d, s, r);
                self.write_modrm_u16(&modrm, bus, r);
                Ok(StepResult::Continue)
            }
            0x02 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let s = self.read_modrm_u8(&modrm, bus);
                let d = self.regs.get8(Self::reg8_from_field(modrm.reg));
                let r = d.wrapping_add(s);
                self.update_add8(d, s, r);
                self.regs.set8(Self::reg8_from_field(modrm.reg), r);
                Ok(StepResult::Continue)
            }
            0x03 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let s = self.read_modrm_u16(&modrm, bus);
                let d = self.regs.get16(Self::reg16_from_field(modrm.reg));
                let r = d.wrapping_add(s);
                self.update_add16(d, s, r);
                self.regs.set16(Self::reg16_from_field(modrm.reg), r);
                Ok(StepResult::Continue)
            }
            // OR
            0x08 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let s = self.regs.get8(Self::reg8_from_field(modrm.reg));
                let d = self.read_modrm_u8(&modrm, bus);
                let r = d | s;
                self.regs.update_flags_u8(r);
                self.regs.set_cf(false);
                self.regs.set_of(false);
                self.write_modrm_u8(&modrm, bus, r);
                Ok(StepResult::Continue)
            }
            0x09 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let s = self.regs.get16(Self::reg16_from_field(modrm.reg));
                let d = self.read_modrm_u16(&modrm, bus);
                let r = d | s;
                self.regs.update_flags_u16(r);
                self.regs.set_cf(false);
                self.regs.set_of(false);
                self.write_modrm_u16(&modrm, bus, r);
                Ok(StepResult::Continue)
            }
            0x0A => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let s = self.read_modrm_u8(&modrm, bus);
                let d = self.regs.get8(Self::reg8_from_field(modrm.reg));
                let r = d | s;
                self.regs.update_flags_u8(r);
                self.regs.set_cf(false);
                self.regs.set_of(false);
                self.regs.set8(Self::reg8_from_field(modrm.reg), r);
                Ok(StepResult::Continue)
            }
            0x0B => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let s = self.read_modrm_u16(&modrm, bus);
                let d = self.regs.get16(Self::reg16_from_field(modrm.reg));
                let r = d | s;
                self.regs.update_flags_u16(r);
                self.regs.set_cf(false);
                self.regs.set_of(false);
                self.regs.set16(Self::reg16_from_field(modrm.reg), r);
                Ok(StepResult::Continue)
            }
            // ADC
            0x10 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let cf = self.regs.get_cf() as u8;
                let s = self.regs.get8(Self::reg8_from_field(modrm.reg));
                let d = self.read_modrm_u8(&modrm, bus);
                let r = d.wrapping_add(s).wrapping_add(cf);
                self.update_add8(d, s.wrapping_add(cf), r);
                self.write_modrm_u8(&modrm, bus, r);
                Ok(StepResult::Continue)
            }
            0x11 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let cf = self.regs.get_cf() as u16;
                let s = self.regs.get16(Self::reg16_from_field(modrm.reg));
                let d = self.read_modrm_u16(&modrm, bus);
                let r = d.wrapping_add(s).wrapping_add(cf);
                self.update_add16(d, s.wrapping_add(cf), r);
                self.write_modrm_u16(&modrm, bus, r);
                Ok(StepResult::Continue)
            }
            0x12 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let cf = self.regs.get_cf() as u8;
                let s = self.read_modrm_u8(&modrm, bus);
                let d = self.regs.get8(Self::reg8_from_field(modrm.reg));
                let r = d.wrapping_add(s).wrapping_add(cf);
                self.update_add8(d, s.wrapping_add(cf), r);
                self.regs.set8(Self::reg8_from_field(modrm.reg), r);
                Ok(StepResult::Continue)
            }
            0x13 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let cf = self.regs.get_cf() as u16;
                let s = self.read_modrm_u16(&modrm, bus);
                let d = self.regs.get16(Self::reg16_from_field(modrm.reg));
                let r = d.wrapping_add(s).wrapping_add(cf);
                self.update_add16(d, s.wrapping_add(cf), r);
                self.regs.set16(Self::reg16_from_field(modrm.reg), r);
                Ok(StepResult::Continue)
            }
            // SBB
            0x18 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let cf = self.regs.get_cf() as u8;
                let s = self.regs.get8(Self::reg8_from_field(modrm.reg));
                let d = self.read_modrm_u8(&modrm, bus);
                let r = d.wrapping_sub(s).wrapping_sub(cf);
                self.update_sub8(d, s.wrapping_add(cf), r);
                self.write_modrm_u8(&modrm, bus, r);
                Ok(StepResult::Continue)
            }
            0x19 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let cf = self.regs.get_cf() as u16;
                let s = self.regs.get16(Self::reg16_from_field(modrm.reg));
                let d = self.read_modrm_u16(&modrm, bus);
                let r = d.wrapping_sub(s).wrapping_sub(cf);
                self.update_sub16(d, s.wrapping_add(cf), r);
                self.write_modrm_u16(&modrm, bus, r);
                Ok(StepResult::Continue)
            }
            0x1A => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let cf = self.regs.get_cf() as u8;
                let s = self.read_modrm_u8(&modrm, bus);
                let d = self.regs.get8(Self::reg8_from_field(modrm.reg));
                let r = d.wrapping_sub(s).wrapping_sub(cf);
                self.update_sub8(d, s.wrapping_add(cf), r);
                self.regs.set8(Self::reg8_from_field(modrm.reg), r);
                Ok(StepResult::Continue)
            }
            0x1B => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let cf = self.regs.get_cf() as u16;
                let s = self.read_modrm_u16(&modrm, bus);
                let d = self.regs.get16(Self::reg16_from_field(modrm.reg));
                let r = d.wrapping_sub(s).wrapping_sub(cf);
                self.update_sub16(d, s.wrapping_add(cf), r);
                self.regs.set16(Self::reg16_from_field(modrm.reg), r);
                Ok(StepResult::Continue)
            }
            // AND
            0x20 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let s = self.regs.get8(Self::reg8_from_field(modrm.reg));
                let d = self.read_modrm_u8(&modrm, bus);
                let r = d & s;
                self.regs.update_flags_u8(r);
                self.regs.set_cf(false);
                self.regs.set_of(false);
                self.write_modrm_u8(&modrm, bus, r);
                Ok(StepResult::Continue)
            }
            0x21 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let s = self.regs.get16(Self::reg16_from_field(modrm.reg));
                let d = self.read_modrm_u16(&modrm, bus);
                let r = d & s;
                self.regs.update_flags_u16(r);
                self.regs.set_cf(false);
                self.regs.set_of(false);
                self.write_modrm_u16(&modrm, bus, r);
                Ok(StepResult::Continue)
            }
            0x22 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let s = self.read_modrm_u8(&modrm, bus);
                let d = self.regs.get8(Self::reg8_from_field(modrm.reg));
                let r = d & s;
                self.regs.update_flags_u8(r);
                self.regs.set_cf(false);
                self.regs.set_of(false);
                self.regs.set8(Self::reg8_from_field(modrm.reg), r);
                Ok(StepResult::Continue)
            }
            0x23 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let s = self.read_modrm_u16(&modrm, bus);
                let d = self.regs.get16(Self::reg16_from_field(modrm.reg));
                let r = d & s;
                self.regs.update_flags_u16(r);
                self.regs.set_cf(false);
                self.regs.set_of(false);
                self.regs.set16(Self::reg16_from_field(modrm.reg), r);
                Ok(StepResult::Continue)
            }
            // SUB
            0x28 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let s = self.regs.get8(Self::reg8_from_field(modrm.reg));
                let d = self.read_modrm_u8(&modrm, bus);
                let r = d.wrapping_sub(s);
                self.update_sub8(d, s, r);
                self.write_modrm_u8(&modrm, bus, r);
                Ok(StepResult::Continue)
            }
            0x29 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let s = self.regs.get16(Self::reg16_from_field(modrm.reg));
                let d = self.read_modrm_u16(&modrm, bus);
                let r = d.wrapping_sub(s);
                self.update_sub16(d, s, r);
                self.write_modrm_u16(&modrm, bus, r);
                Ok(StepResult::Continue)
            }
            0x2A => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let s = self.read_modrm_u8(&modrm, bus);
                let d = self.regs.get8(Self::reg8_from_field(modrm.reg));
                let r = d.wrapping_sub(s);
                self.update_sub8(d, s, r);
                self.regs.set8(Self::reg8_from_field(modrm.reg), r);
                Ok(StepResult::Continue)
            }
            0x2B => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let s = self.read_modrm_u16(&modrm, bus);
                let d = self.regs.get16(Self::reg16_from_field(modrm.reg));
                let r = d.wrapping_sub(s);
                self.update_sub16(d, s, r);
                self.regs.set16(Self::reg16_from_field(modrm.reg), r);
                Ok(StepResult::Continue)
            }
            // XOR
            0x30 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let s = self.regs.get8(Self::reg8_from_field(modrm.reg));
                let d = self.read_modrm_u8(&modrm, bus);
                let r = d ^ s;
                self.regs.update_flags_u8(r);
                self.regs.set_cf(false);
                self.regs.set_of(false);
                self.write_modrm_u8(&modrm, bus, r);
                Ok(StepResult::Continue)
            }
            0x31 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let s = self.regs.get16(Self::reg16_from_field(modrm.reg));
                let d = self.read_modrm_u16(&modrm, bus);
                let r = d ^ s;
                self.regs.update_flags_u16(r);
                self.regs.set_cf(false);
                self.regs.set_of(false);
                self.write_modrm_u16(&modrm, bus, r);
                Ok(StepResult::Continue)
            }
            0x32 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let s = self.read_modrm_u8(&modrm, bus);
                let d = self.regs.get8(Self::reg8_from_field(modrm.reg));
                let r = d ^ s;
                self.regs.update_flags_u8(r);
                self.regs.set_cf(false);
                self.regs.set_of(false);
                self.regs.set8(Self::reg8_from_field(modrm.reg), r);
                Ok(StepResult::Continue)
            }
            0x33 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let s = self.read_modrm_u16(&modrm, bus);
                let d = self.regs.get16(Self::reg16_from_field(modrm.reg));
                let r = d ^ s;
                self.regs.update_flags_u16(r);
                self.regs.set_cf(false);
                self.regs.set_of(false);
                self.regs.set16(Self::reg16_from_field(modrm.reg), r);
                Ok(StepResult::Continue)
            }
            // CMP
            0x38 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let s = self.regs.get8(Self::reg8_from_field(modrm.reg));
                let d = self.read_modrm_u8(&modrm, bus);
                let r = d.wrapping_sub(s);
                self.update_sub8(d, s, r);
                Ok(StepResult::Continue)
            }
            0x39 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let s = self.regs.get16(Self::reg16_from_field(modrm.reg));
                let d = self.read_modrm_u16(&modrm, bus);
                let r = d.wrapping_sub(s);
                self.update_sub16(d, s, r);
                Ok(StepResult::Continue)
            }
            0x3A => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let s = self.read_modrm_u8(&modrm, bus);
                let d = self.regs.get8(Self::reg8_from_field(modrm.reg));
                let r = d.wrapping_sub(s);
                self.update_sub8(d, s, r);
                Ok(StepResult::Continue)
            }
            0x3B => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let s = self.read_modrm_u16(&modrm, bus);
                let d = self.regs.get16(Self::reg16_from_field(modrm.reg));
                let r = d.wrapping_sub(s);
                self.update_sub16(d, s, r);
                Ok(StepResult::Continue)
            }

            // INC r16 (0x40-0x47)
            0x40..=0x47 => {
                let reg = Self::reg16_from_field(opcode & 7);
                let v = self.regs.get16(reg);
                let r = v.wrapping_add(1);
                let old_cf = self.regs.get_cf();
                self.regs.update_flags_u16(r);
                self.regs.set_of(v == 0x7FFF);
                self.regs.set_cf(old_cf);
                self.regs.set16(reg, r);
                Ok(StepResult::Continue)
            }

            // DEC r16 (0x48-0x4F)
            0x48..=0x4F => {
                let reg = Self::reg16_from_field(opcode & 7);
                let v = self.regs.get16(reg);
                let r = v.wrapping_sub(1);
                let old_cf = self.regs.get_cf();
                self.regs.update_flags_u16(r);
                self.regs.set_of(v == 0x8000);
                self.regs.set_cf(old_cf);
                self.regs.set16(reg, r);
                Ok(StepResult::Continue)
            }

            // PUSH r16 (0x50-0x57)
            0x50..=0x57 => {
                let reg = Self::reg16_from_field(opcode & 7);
                let val = self.regs.get16(reg);
                self.push16(bus, val);
                Ok(StepResult::Continue)
            }

            // POP r16 (0x58-0x5F)
            0x58..=0x5F => {
                let reg = Self::reg16_from_field(opcode & 7);
                let val = self.pop16(bus);
                self.regs.set16(reg, val);
                Ok(StepResult::Continue)
            }

            // PUSH/POP segment regs
            0x06 => {
                let v = self.regs.get_seg(SegReg::ES);
                self.push16(bus, v);
                Ok(StepResult::Continue)
            }
            0x0E => {
                let v = self.regs.get_seg(SegReg::CS);
                self.push16(bus, v);
                Ok(StepResult::Continue)
            }
            0x16 => {
                let v = self.regs.get_seg(SegReg::SS);
                self.push16(bus, v);
                Ok(StepResult::Continue)
            }
            0x1E => {
                let v = self.regs.get_seg(SegReg::DS);
                self.push16(bus, v);
                Ok(StepResult::Continue)
            }
            0x07 => {
                let v = self.pop16(bus);
                self.regs.set_seg(SegReg::ES, v);
                Ok(StepResult::Continue)
            }
            0x17 => {
                let v = self.pop16(bus);
                self.regs.set_seg(SegReg::SS, v);
                Ok(StepResult::Continue)
            }
            0x1F => {
                let v = self.pop16(bus);
                self.regs.set_seg(SegReg::DS, v);
                Ok(StepResult::Continue)
            }

            // PUSHA / POPA
            0x60 => {
                let sp_orig = self.regs.get16(Reg16::SP);
                let ax = self.regs.get16(Reg16::AX);
                let cx = self.regs.get16(Reg16::CX);
                let dx = self.regs.get16(Reg16::DX);
                let bx = self.regs.get16(Reg16::BX);
                let bp = self.regs.get16(Reg16::BP);
                let si = self.regs.get16(Reg16::SI);
                let di = self.regs.get16(Reg16::DI);
                self.push16(bus, ax);
                self.push16(bus, cx);
                self.push16(bus, dx);
                self.push16(bus, bx);
                self.push16(bus, sp_orig);
                self.push16(bus, bp);
                self.push16(bus, si);
                self.push16(bus, di);
                Ok(StepResult::Continue)
            }
            0x61 => {
                let di = self.pop16(bus);
                self.regs.set16(Reg16::DI, di);
                let si = self.pop16(bus);
                self.regs.set16(Reg16::SI, si);
                let bp = self.pop16(bus);
                self.regs.set16(Reg16::BP, bp);
                let _sp = self.pop16(bus);
                let bx = self.pop16(bus);
                self.regs.set16(Reg16::BX, bx);
                let dx = self.pop16(bus);
                self.regs.set16(Reg16::DX, dx);
                let cx = self.pop16(bus);
                self.regs.set16(Reg16::CX, cx);
                let ax = self.pop16(bus);
                self.regs.set16(Reg16::AX, ax);
                Ok(StepResult::Continue)
            }

            // PUSH imm
            0x68 => {
                let imm = self.fetch_u16(bus);
                self.push16(bus, imm);
                Ok(StepResult::Continue)
            }
            0x6A => {
                let imm = self.fetch_i8(bus) as u16;
                self.push16(bus, imm);
                Ok(StepResult::Continue)
            }

            // IMUL r16, r/m16, imm
            0x69 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let src = self.read_modrm_u16(&modrm, bus) as i16;
                let imm = self.fetch_i16(bus);
                let (res, over) = (src as i32).overflowing_mul(imm as i32);
                self.regs
                    .set16(Self::reg16_from_field(modrm.reg), res as u16);
                self.regs.set_cf(over);
                self.regs.set_of(over);
                Ok(StepResult::Continue)
            }
            0x6B => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let src = self.read_modrm_u16(&modrm, bus) as i16;
                let imm = self.fetch_i8(bus) as i32;
                let (res, over) = (src as i32).overflowing_mul(imm);
                self.regs
                    .set16(Self::reg16_from_field(modrm.reg), res as u16);
                self.regs.set_cf(over);
                self.regs.set_of(over);
                Ok(StepResult::Continue)
            }

            // Jcc short (0x70-0x7F)
            0x70..=0x7F => {
                let cond = opcode & 0x0F;
                let rel = self.fetch_i8(bus) as i32;
                if self.check_cond(cond) {
                    self.regs.ip = self.regs.ip.wrapping_add(rel as u32);
                }
                Ok(StepResult::Continue)
            }

            // Group 1: 0x80-0x83
            0x80 | 0x82 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let imm = self.fetch_u8(bus);
                let r = self.group1_u8(bus, &modrm, imm);
                if modrm.reg != 7 {
                    self.write_modrm_u8(&modrm, bus, r);
                }
                Ok(StepResult::Continue)
            }
            0x81 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let imm = self.fetch_u16(bus);
                let r = self.group1_u16(bus, &modrm, imm);
                if modrm.reg != 7 {
                    self.write_modrm_u16(&modrm, bus, r);
                }
                Ok(StepResult::Continue)
            }
            0x83 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let imm = self.fetch_i8(bus) as u16;
                let r = self.group1_u16(bus, &modrm, imm);
                if modrm.reg != 7 {
                    self.write_modrm_u16(&modrm, bus, r);
                }
                Ok(StepResult::Continue)
            }

            // TEST
            0x84 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let s = self.regs.get8(Self::reg8_from_field(modrm.reg));
                let d = self.read_modrm_u8(&modrm, bus);
                let r = d & s;
                self.regs.update_flags_u8(r);
                self.regs.set_cf(false);
                self.regs.set_of(false);
                Ok(StepResult::Continue)
            }
            0x85 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let s = self.regs.get16(Self::reg16_from_field(modrm.reg));
                let d = self.read_modrm_u16(&modrm, bus);
                let r = d & s;
                self.regs.update_flags_u16(r);
                self.regs.set_cf(false);
                self.regs.set_of(false);
                Ok(StepResult::Continue)
            }
            0xA8 => {
                let imm = self.fetch_u8(bus);
                let r = self.regs.get8(Reg8::AL) & imm;
                self.regs.update_flags_u8(r);
                self.regs.set_cf(false);
                self.regs.set_of(false);
                Ok(StepResult::Continue)
            }
            0xA9 => {
                let imm = self.fetch_u16(bus);
                let r = self.regs.get16(Reg16::AX) & imm;
                self.regs.update_flags_u16(r);
                self.regs.set_cf(false);
                self.regs.set_of(false);
                Ok(StepResult::Continue)
            }

            // XCHG
            0x86 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let reg = Self::reg8_from_field(modrm.reg);
                let a = self.regs.get8(reg);
                let b = self.read_modrm_u8(&modrm, bus);
                self.regs.set8(reg, b);
                self.write_modrm_u8(&modrm, bus, a);
                Ok(StepResult::Continue)
            }
            0x87 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let reg = Self::reg16_from_field(modrm.reg);
                let a = self.regs.get16(reg);
                let b = self.read_modrm_u16(&modrm, bus);
                self.regs.set16(reg, b);
                self.write_modrm_u16(&modrm, bus, a);
                Ok(StepResult::Continue)
            }
            // XCHG AX, r16
            0x91..=0x97 => {
                let reg = Self::reg16_from_field(opcode & 7);
                let ax = self.regs.get16(Reg16::AX);
                let rv = self.regs.get16(reg);
                self.regs.set16(Reg16::AX, rv);
                self.regs.set16(reg, ax);
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
                let sreg = Self::seg_from_field(modrm.reg);
                self.regs.set_seg(sreg, val);
                Ok(StepResult::Continue)
            }

            // LEA
            0x8D => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let (_, off) = self.decode_ea(&modrm, bus);
                self.regs
                    .set16(Self::reg16_from_field(modrm.reg), off as u16);
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

            // CBW / CWD
            0x98 => {
                let al = self.regs.get8(Reg8::AL) as i8 as i16 as u16;
                self.regs.set16(Reg16::AX, al);
                Ok(StepResult::Continue)
            }
            0x99 => {
                let ax = self.regs.get16(Reg16::AX) as i16;
                let dx = if ax < 0 { 0xFFFFu16 } else { 0 };
                self.regs.set16(Reg16::DX, dx);
                Ok(StepResult::Continue)
            }

            // CALL far / NOP
            0x9A => {
                let off = self.fetch_u16(bus);
                let seg = self.fetch_u16(bus);
                let ret_cs = self.regs.get_seg(SegReg::CS);
                let ret_ip = self.regs.ip as u16;
                self.push16(bus, ret_cs);
                self.push16(bus, ret_ip);
                self.regs.set_seg(SegReg::CS, seg);
                self.regs.ip = off as u32;
                Ok(StepResult::Continue)
            }

            // WAIT / PUSHF / POPF / SAHF / LAHF
            0x9B => Ok(StepResult::Continue),
            0x9C => {
                let f = self.regs.flags as u16;
                self.push16(bus, f);
                Ok(StepResult::Continue)
            }
            0x9D => {
                let f = self.pop16(bus);
                self.regs.flags = (self.regs.flags & !0xFFFF) | (f as u32) | 0x0002;
                Ok(StepResult::Continue)
            }
            0x9E => {
                let ah = self.regs.get8(Reg8::AH) as u32;
                self.regs.flags = (self.regs.flags & !0xFF) | (ah & 0xD5) | 0x02;
                Ok(StepResult::Continue)
            }
            0x9F => {
                let f = (self.regs.flags & 0xFF) as u8;
                self.regs.set8(Reg8::AH, f);
                Ok(StepResult::Continue)
            }

            // MOV moffset
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

            // String ops (standalone)
            0xA4 => {
                self.movsb(bus);
                Ok(StepResult::Continue)
            }
            0xA5 => {
                self.movsw(bus);
                Ok(StepResult::Continue)
            }
            0xA6 => {
                let si = self.regs.get16(Reg16::SI);
                let di = self.regs.get16(Reg16::DI);
                let seg = self.prefix.seg_override.unwrap_or(SegReg::DS);
                let a = bus.mem.seg_read_u8(self.regs.get_seg(seg), si as u32);
                let b = bus
                    .mem
                    .seg_read_u8(self.regs.get_seg(SegReg::ES), di as u32);
                let r = a.wrapping_sub(b);
                self.update_sub8(a, b, r);
                self.str_si_advance(1);
                self.str_di_advance(1);
                Ok(StepResult::Continue)
            }
            0xA7 => {
                let si = self.regs.get16(Reg16::SI);
                let di = self.regs.get16(Reg16::DI);
                let seg = self.prefix.seg_override.unwrap_or(SegReg::DS);
                let a = bus.mem.seg_read_u16(self.regs.get_seg(seg), si as u32);
                let b = bus
                    .mem
                    .seg_read_u16(self.regs.get_seg(SegReg::ES), di as u32);
                let r = a.wrapping_sub(b);
                self.update_sub16(a, b, r);
                self.str_si_advance(2);
                self.str_di_advance(2);
                Ok(StepResult::Continue)
            }
            0xAA => {
                self.stosb(bus);
                Ok(StepResult::Continue)
            }
            0xAB => {
                self.stosw(bus);
                Ok(StepResult::Continue)
            }
            0xAC => {
                self.lodsb(bus);
                Ok(StepResult::Continue)
            }
            0xAD => {
                self.lodsw(bus);
                Ok(StepResult::Continue)
            }
            0xAE => {
                self.scasb(bus);
                Ok(StepResult::Continue)
            }
            0xAF => {
                self.scasw(bus);
                Ok(StepResult::Continue)
            }

            // MOV r, imm
            0xB0..=0xB7 => {
                let reg = Self::reg8_from_field(opcode & 7);
                let imm = self.fetch_u8(bus);
                self.regs.set8(reg, imm);
                Ok(StepResult::Continue)
            }
            0xB8..=0xBF => {
                let reg = Self::reg16_from_field(opcode & 7);
                let imm = self.fetch_u16(bus);
                self.regs.set16(reg, imm);
                Ok(StepResult::Continue)
            }

            // Shift Group 2
            0xD0 => {
                let mb = self.fetch_u8(bus);
                self.shift_rm8(bus, mb, 1);
                Ok(StepResult::Continue)
            }
            0xD1 => {
                let mb = self.fetch_u8(bus);
                self.shift_rm16(bus, mb, 1);
                Ok(StepResult::Continue)
            }
            0xD2 => {
                let mb = self.fetch_u8(bus);
                let cnt = self.regs.get8(Reg8::CL);
                self.shift_rm8(bus, mb, cnt);
                Ok(StepResult::Continue)
            }
            0xD3 => {
                let mb = self.fetch_u8(bus);
                let cnt = self.regs.get8(Reg8::CL);
                self.shift_rm16(bus, mb, cnt);
                Ok(StepResult::Continue)
            }
            // Shift r/m8 / r/m16 with imm8 count
            0xC0 => {
                let mb = self.fetch_u8(bus);
                let cnt = self.fetch_u8(bus);
                self.shift_rm8(bus, mb, cnt);
                Ok(StepResult::Continue)
            }
            0xC1 => {
                let mb = self.fetch_u8(bus);
                let cnt = self.fetch_u8(bus);
                self.shift_rm16(bus, mb, cnt);
                Ok(StepResult::Continue)
            }

            // ENTER / LEAVE
            0xC8 => {
                let alloc = self.fetch_u16(bus);
                let level = self.fetch_u8(bus) & 0x1F;
                let bp = self.regs.get16(Reg16::BP);
                self.push16(bus, bp);
                let frame_ptr = self.regs.get16(Reg16::SP);
                if level > 0 {
                    for i in 1..level {
                        let bp_cur = self.regs.get16(Reg16::BP);
                        let tmp = bus.mem.seg_read_u16(
                            self.regs.get_seg(SegReg::SS),
                            bp_cur.wrapping_sub(i as u16 * 2) as u32,
                        );
                        self.push16(bus, tmp);
                    }
                    self.push16(bus, frame_ptr);
                }
                self.regs.set16(Reg16::BP, frame_ptr);
                let sp = self.regs.get16(Reg16::SP).wrapping_sub(alloc);
                self.regs.set16(Reg16::SP, sp);
                Ok(StepResult::Continue)
            }
            0xC9 => {
                let bp = self.regs.get16(Reg16::BP);
                self.regs.set16(Reg16::SP, bp);
                let new_bp = self.pop16(bus);
                self.regs.set16(Reg16::BP, new_bp);
                Ok(StepResult::Continue)
            }

            // RET
            0xC2 => {
                let n = self.fetch_u16(bus);
                let ip = self.pop16(bus);
                self.regs.ip = ip as u32;
                let sp = self.regs.get16(Reg16::SP).wrapping_add(n);
                self.regs.set16(Reg16::SP, sp);
                Ok(StepResult::Continue)
            }
            0xC3 => {
                let ip = self.pop16(bus);
                self.regs.ip = ip as u32;
                Ok(StepResult::Continue)
            }
            0xCA => {
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
                let ip = self.pop16(bus);
                let cs = self.pop16(bus);
                self.regs.ip = ip as u32;
                self.regs.set_seg(SegReg::CS, cs);
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

            // LDS / LES / LFS / LGS
            0xC4 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let ea = self.ea_from_modrm(&modrm, bus);
                let off = bus.mem.read_u16(ea);
                let seg = bus.mem.read_u16(ea + 2);
                self.regs.set16(Self::reg16_from_field(modrm.reg), off);
                self.regs.set_seg(SegReg::ES, seg);
                Ok(StepResult::Continue)
            }
            0xC5 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                let ea = self.ea_from_modrm(&modrm, bus);
                let off = bus.mem.read_u16(ea);
                let seg = bus.mem.read_u16(ea + 2);
                self.regs.set16(Self::reg16_from_field(modrm.reg), off);
                self.regs.set_seg(SegReg::DS, seg);
                Ok(StepResult::Continue)
            }

            // AAM / AAD / BCD stubs
            0xD4 | 0xD5 => {
                self.fetch_u8(bus);
                Ok(StepResult::Continue)
            }
            0x27 | 0x2F | 0x37 | 0x3F => Ok(StepResult::Continue),

            // XLAT
            0xD7 => {
                let bx = self.regs.get16(Reg16::BX);
                let al = self.regs.get8(Reg8::AL) as u16;
                let v = bus
                    .mem
                    .seg_read_u8(self.regs.get_seg(SegReg::DS), bx.wrapping_add(al) as u32);
                self.regs.set8(Reg8::AL, v);
                Ok(StepResult::Continue)
            }

            // FPU stubs
            0xD8..=0xDF => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                if modrm.mode != 3 {
                    self.decode_ea(&modrm, bus);
                }
                Ok(StepResult::Continue)
            }

            // LOOP / LOOPE / LOOPNE / JCXZ
            0xE0 => {
                let rel = self.fetch_i8(bus) as i32;
                let cx = self.regs.get16(Reg16::CX).wrapping_sub(1);
                self.regs.set16(Reg16::CX, cx);
                if cx != 0 && !self.regs.get_zf() {
                    self.regs.ip = self.regs.ip.wrapping_add(rel as u32);
                }
                Ok(StepResult::Continue)
            }
            0xE1 => {
                let rel = self.fetch_i8(bus) as i32;
                let cx = self.regs.get16(Reg16::CX).wrapping_sub(1);
                self.regs.set16(Reg16::CX, cx);
                if cx != 0 && self.regs.get_zf() {
                    self.regs.ip = self.regs.ip.wrapping_add(rel as u32);
                }
                Ok(StepResult::Continue)
            }
            0xE2 => {
                let rel = self.fetch_i8(bus) as i32;
                let cx = self.regs.get16(Reg16::CX).wrapping_sub(1);
                self.regs.set16(Reg16::CX, cx);
                if cx != 0 {
                    self.regs.ip = self.regs.ip.wrapping_add(rel as u32);
                }
                Ok(StepResult::Continue)
            }
            0xE3 => {
                let rel = self.fetch_i8(bus) as i32;
                if self.regs.get16(Reg16::CX) == 0 {
                    self.regs.ip = self.regs.ip.wrapping_add(rel as u32);
                }
                Ok(StepResult::Continue)
            }

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

            // INSB / INSW / OUTSB / OUTSW
            0x6C => {
                // INSB: read byte from port DX → ES:DI, advance DI
                let di = self.regs.get16(Reg16::DI);
                bus.mem.seg_write_u8(self.regs.get_seg(SegReg::ES), di as u32, 0xFF);
                self.str_di_advance(1);
                Ok(StepResult::Continue)
            }
            0x6D => {
                // INSW: read word from port DX → ES:DI, advance DI
                let di = self.regs.get16(Reg16::DI);
                bus.mem.seg_write_u16(self.regs.get_seg(SegReg::ES), di as u32, 0xFFFF);
                self.str_di_advance(2);
                Ok(StepResult::Continue)
            }
            0x6E => {
                // OUTSB: write byte from DS:SI to port DX, advance SI
                self.str_si_advance(1);
                Ok(StepResult::Continue)
            }
            0x6F => {
                // OUTSW: write word from DS:SI to port DX, advance SI
                self.str_si_advance(2);
                Ok(StepResult::Continue)
            }

            // CALL / JMP
            0xE8 => {
                let rel = self.fetch_i16(bus) as i32;
                let ret_ip = self.regs.ip as u16;
                self.push16(bus, ret_ip);
                self.regs.ip = self.regs.ip.wrapping_add(rel as u32);
                Ok(StepResult::Continue)
            }
            0xE9 => {
                let rel = self.fetch_i16(bus) as i32;
                self.regs.ip = self.regs.ip.wrapping_add(rel as u32);
                Ok(StepResult::Continue)
            }
            0xEA => {
                let off = self.fetch_u16(bus);
                let seg = self.fetch_u16(bus);
                self.regs.ip = off as u32;
                self.regs.set_seg(SegReg::CS, seg);
                Ok(StepResult::Continue)
            }
            0xEB => {
                let rel = self.fetch_i8(bus) as i32;
                self.regs.ip = self.regs.ip.wrapping_add(rel as u32);
                Ok(StepResult::Continue)
            }

            // REP / REPNE
            0xF2 => {
                // Skip and handle any prefix bytes (LOCK, segment overrides, size prefixes)
                let mut next = self.fetch_u8(bus);
                loop {
                    match next {
                        0x26 => self.prefix.seg_override = Some(crate::cpu::regs::SegReg::ES),
                        0x2E => self.prefix.seg_override = Some(crate::cpu::regs::SegReg::CS),
                        0x36 => self.prefix.seg_override = Some(crate::cpu::regs::SegReg::SS),
                        0x3E => self.prefix.seg_override = Some(crate::cpu::regs::SegReg::DS),
                        0x64 => self.prefix.seg_override = Some(crate::cpu::regs::SegReg::FS),
                        0x65 => self.prefix.seg_override = Some(crate::cpu::regs::SegReg::GS),
                        0x66 => self.prefix.operand_32 = !self.prefix.operand_32,
                        0x67 => self.prefix.address_32 = !self.prefix.address_32,
                        0xF0 => {} // LOCK
                        _ => break,
                    }
                    next = self.fetch_u8(bus);
                }
                // REPNE only applies to SCAS/CMPS; any other opcode executes once
                match next {
                    0xAE | 0xAF => {
                        loop {
                            let cx = self.regs.get16(Reg16::CX);
                            if cx == 0 { break; }
                            self.regs.set16(Reg16::CX, cx.wrapping_sub(1));
                            match next {
                                0xAE => self.scasb(bus),
                                0xAF => self.scasw(bus),
                                _ => unreachable!(),
                            }
                            if self.regs.get_zf() { break; }
                        }
                        Ok(StepResult::Continue)
                    }
                    _ => self.dispatch(next, bus, ip_before),
                }
            }
            0xF3 => {
                // Skip and handle any prefix bytes (LOCK, segment overrides, size prefixes)
                let mut next = self.fetch_u8(bus);
                loop {
                    match next {
                        0x26 => self.prefix.seg_override = Some(crate::cpu::regs::SegReg::ES),
                        0x2E => self.prefix.seg_override = Some(crate::cpu::regs::SegReg::CS),
                        0x36 => self.prefix.seg_override = Some(crate::cpu::regs::SegReg::SS),
                        0x3E => self.prefix.seg_override = Some(crate::cpu::regs::SegReg::DS),
                        0x64 => self.prefix.seg_override = Some(crate::cpu::regs::SegReg::FS),
                        0x65 => self.prefix.seg_override = Some(crate::cpu::regs::SegReg::GS),
                        0x66 => self.prefix.operand_32 = !self.prefix.operand_32,
                        0x67 => self.prefix.address_32 = !self.prefix.address_32,
                        0xF0 => {} // LOCK
                        _ => break,
                    }
                    next = self.fetch_u8(bus);
                }
                // REP only applies to string ops; any other opcode executes once
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
                                0xA6 => {
                                    let si = self.regs.get16(Reg16::SI);
                                    let di = self.regs.get16(Reg16::DI);
                                    let seg = self.prefix.seg_override.unwrap_or(SegReg::DS);
                                    let a = bus.mem.seg_read_u8(self.regs.get_seg(seg), si as u32);
                                    let b = bus.mem.seg_read_u8(self.regs.get_seg(SegReg::ES), di as u32);
                                    let r = a.wrapping_sub(b);
                                    self.update_sub8(a, b, r);
                                    self.str_si_advance(1);
                                    self.str_di_advance(1);
                                    if !self.regs.get_zf() { break; }
                                }
                                0xA7 => {
                                    let si = self.regs.get16(Reg16::SI);
                                    let di = self.regs.get16(Reg16::DI);
                                    let seg = self.prefix.seg_override.unwrap_or(SegReg::DS);
                                    let a = bus.mem.seg_read_u16(self.regs.get_seg(seg), si as u32);
                                    let b = bus.mem.seg_read_u16(self.regs.get_seg(SegReg::ES), di as u32);
                                    let r = a.wrapping_sub(b);
                                    self.update_sub16(a, b, r);
                                    self.str_si_advance(2);
                                    self.str_di_advance(2);
                                    if !self.regs.get_zf() { break; }
                                }
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

            // Group FE: INC/DEC r/m8
            0xFE => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                match modrm.reg {
                    0 => {
                        let v = self.read_modrm_u8(&modrm, bus);
                        let r = v.wrapping_add(1);
                        let old_cf = self.regs.get_cf();
                        self.regs.update_flags_u8(r);
                        self.regs.set_of(v == 0x7F);
                        self.regs.set_cf(old_cf);
                        self.write_modrm_u8(&modrm, bus, r);
                    }
                    1 => {
                        let v = self.read_modrm_u8(&modrm, bus);
                        let r = v.wrapping_sub(1);
                        let old_cf = self.regs.get_cf();
                        self.regs.update_flags_u8(r);
                        self.regs.set_of(v == 0x80);
                        self.regs.set_cf(old_cf);
                        self.write_modrm_u8(&modrm, bus, r);
                    }
                    _ => log::warn!("Unimplemented FE /{}", modrm.reg),
                }
                Ok(StepResult::Continue)
            }

            // Group FF: INC/DEC/CALL/JMP/PUSH r/m16
            0xFF => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                match modrm.reg {
                    0 => {
                        let v = self.read_modrm_u16(&modrm, bus);
                        let r = v.wrapping_add(1);
                        let old_cf = self.regs.get_cf();
                        self.regs.update_flags_u16(r);
                        self.regs.set_of(v == 0x7FFF);
                        self.regs.set_cf(old_cf);
                        self.write_modrm_u16(&modrm, bus, r);
                    }
                    1 => {
                        let v = self.read_modrm_u16(&modrm, bus);
                        let r = v.wrapping_sub(1);
                        let old_cf = self.regs.get_cf();
                        self.regs.update_flags_u16(r);
                        self.regs.set_of(v == 0x8000);
                        self.regs.set_cf(old_cf);
                        self.write_modrm_u16(&modrm, bus, r);
                    }
                    2 => {
                        let target = self.read_modrm_u16(&modrm, bus);
                        let ret_ip = self.regs.ip as u16;
                        self.push16(bus, ret_ip);
                        self.regs.ip = target as u32;
                    }
                    3 => {
                        let ea = self.ea_from_modrm(&modrm, bus);
                        let off = bus.mem.read_u16(ea);
                        let seg = bus.mem.read_u16(ea + 2);
                        let ret_cs = self.regs.get_seg(SegReg::CS);
                        let ret_ip = self.regs.ip as u16;
                        self.push16(bus, ret_cs);
                        self.push16(bus, ret_ip);
                        self.regs.set_seg(SegReg::CS, seg);
                        self.regs.ip = off as u32;
                    }
                    4 => {
                        let target = self.read_modrm_u16(&modrm, bus);
                        self.regs.ip = target as u32;
                    }
                    5 => {
                        let ea = self.ea_from_modrm(&modrm, bus);
                        let off = bus.mem.read_u16(ea);
                        let seg = bus.mem.read_u16(ea + 2);
                        self.regs.ip = off as u32;
                        self.regs.set_seg(SegReg::CS, seg);
                    }
                    6 => {
                        let val = self.read_modrm_u16(&modrm, bus);
                        self.push16(bus, val);
                    }
                    _ => {
                        log::warn!(
                            "Unimplemented FF /{} at {:04X}:{:04X} (phys {:05X})",
                            modrm.reg,
                            self.regs.get_seg(SegReg::CS),
                            ip_before,
                            crate::memory::Memory::phys(self.regs.get_seg(SegReg::CS), ip_before as u32)
                        );
                        if modrm.mode != 3 {
                            self.decode_ea(&modrm, bus);
                        }
                    }
                }
                Ok(StepResult::Continue)
            }

            // Group F6: TEST/NOT/NEG/MUL/IMUL/DIV/IDIV r/m8
            0xF6 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                match modrm.reg {
                    0 | 1 => {
                        let imm = self.fetch_u8(bus);
                        let d = self.read_modrm_u8(&modrm, bus);
                        let r = d & imm;
                        self.regs.update_flags_u8(r);
                        self.regs.set_cf(false);
                        self.regs.set_of(false);
                    }
                    2 => {
                        let v = self.read_modrm_u8(&modrm, bus);
                        self.write_modrm_u8(&modrm, bus, !v);
                    }
                    3 => {
                        let v = self.read_modrm_u8(&modrm, bus);
                        let r = 0u8.wrapping_sub(v);
                        self.update_sub8(0, v, r);
                        self.write_modrm_u8(&modrm, bus, r);
                    }
                    4 => {
                        let src = self.read_modrm_u8(&modrm, bus) as u16;
                        let al = self.regs.get8(Reg8::AL) as u16;
                        let r = al * src;
                        self.regs.set16(Reg16::AX, r);
                        let hi = r >> 8 != 0;
                        self.regs.set_cf(hi);
                        self.regs.set_of(hi);
                    }
                    5 => {
                        let src = self.read_modrm_u8(&modrm, bus) as i8 as i16;
                        let al = self.regs.get8(Reg8::AL) as i8 as i16;
                        let r = al * src;
                        self.regs.set16(Reg16::AX, r as u16);
                        let ov = r != (r as i8 as i16);
                        self.regs.set_cf(ov);
                        self.regs.set_of(ov);
                    }
                    6 => {
                        let src = self.read_modrm_u8(&modrm, bus) as u16;
                        if src == 0 {
                            return Ok(StepResult::Interrupt(0));
                        }
                        let ax = self.regs.get16(Reg16::AX);
                        let q = ax / src;
                        let r = ax % src;
                        if q > 0xFF {
                            return Ok(StepResult::Interrupt(0));
                        }
                        self.regs.set8(Reg8::AL, q as u8);
                        self.regs.set8(Reg8::AH, r as u8);
                    }
                    7 => {
                        let src = self.read_modrm_u8(&modrm, bus) as i8 as i16;
                        if src == 0 {
                            return Ok(StepResult::Interrupt(0));
                        }
                        let ax = self.regs.get16(Reg16::AX) as i16;
                        let q = ax / src;
                        let r = ax % src;
                        if q > 127 || q < -128 {
                            return Ok(StepResult::Interrupt(0));
                        }
                        self.regs.set8(Reg8::AL, q as u8);
                        self.regs.set8(Reg8::AH, r as u8);
                    }
                    _ => unreachable!(),
                }
                Ok(StepResult::Continue)
            }

            // Group F7: TEST/NOT/NEG/MUL/IMUL/DIV/IDIV r/m16
            0xF7 => {
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                match modrm.reg {
                    0 | 1 => {
                        let imm = self.fetch_u16(bus);
                        let d = self.read_modrm_u16(&modrm, bus);
                        let r = d & imm;
                        self.regs.update_flags_u16(r);
                        self.regs.set_cf(false);
                        self.regs.set_of(false);
                    }
                    2 => {
                        let v = self.read_modrm_u16(&modrm, bus);
                        self.write_modrm_u16(&modrm, bus, !v);
                    }
                    3 => {
                        let v = self.read_modrm_u16(&modrm, bus);
                        let r = 0u16.wrapping_sub(v);
                        self.update_sub16(0, v, r);
                        self.write_modrm_u16(&modrm, bus, r);
                    }
                    4 => {
                        let src = self.read_modrm_u16(&modrm, bus) as u32;
                        let ax = self.regs.get16(Reg16::AX) as u32;
                        let r = ax * src;
                        self.regs.set16(Reg16::AX, r as u16);
                        self.regs.set16(Reg16::DX, (r >> 16) as u16);
                        let hi = r >> 16 != 0;
                        self.regs.set_cf(hi);
                        self.regs.set_of(hi);
                    }
                    5 => {
                        let src = self.read_modrm_u16(&modrm, bus) as i16 as i32;
                        let ax = self.regs.get16(Reg16::AX) as i16 as i32;
                        let r = ax * src;
                        self.regs.set16(Reg16::AX, r as u16);
                        self.regs.set16(Reg16::DX, (r >> 16) as u16);
                        let ov = r != (r as i16 as i32);
                        self.regs.set_cf(ov);
                        self.regs.set_of(ov);
                    }
                    6 => {
                        let src = self.read_modrm_u16(&modrm, bus) as u32;
                        if src == 0 {
                            return Ok(StepResult::Interrupt(0));
                        }
                        let dxax = ((self.regs.get16(Reg16::DX) as u32) << 16)
                            | self.regs.get16(Reg16::AX) as u32;
                        let q = dxax / src;
                        let r = dxax % src;
                        if q > 0xFFFF {
                            return Ok(StepResult::Interrupt(0));
                        }
                        self.regs.set16(Reg16::AX, q as u16);
                        self.regs.set16(Reg16::DX, r as u16);
                    }
                    7 => {
                        let src = self.read_modrm_u16(&modrm, bus) as i16 as i64;
                        if src == 0 {
                            return Ok(StepResult::Interrupt(0));
                        }
                        let dxax = (((self.regs.get16(Reg16::DX) as i16 as i64) << 16)
                            | self.regs.get16(Reg16::AX) as i64)
                            as i64;
                        let q = dxax / src;
                        let r = dxax % src;
                        if q > 32767 || q < -32768 {
                            return Ok(StepResult::Interrupt(0));
                        }
                        self.regs.set16(Reg16::AX, q as u16);
                        self.regs.set16(Reg16::DX, r as u16);
                    }
                    _ => unreachable!(),
                }
                Ok(StepResult::Continue)
            }

            _ => {
                log::error!(
                    "Unimplemented opcode {:02X} at {:04X}:{:04X} — halting",
                    opcode,
                    self.regs.get_seg(SegReg::CS),
                    ip_before
                );
                self.halted = true;
                Ok(StepResult::Halt)
            }
        }
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}
