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

        // Security check for execution range
        if !self.valid_ranges.is_empty() {
            let mut valid = false;
            for range in &self.valid_ranges {
                if range.contains(&phys_ip_start) {
                    valid = true;
                    break;
                }
            }
            // Allow BIOS and lower memory access
            if phys_ip_start < 0x500 || phys_ip_start >= 0xF0000 {
                valid = true;
            }

            if !valid {
                log::error!(
                    "Execution jump to invalid memory at {:04X}:{:08X}",
                    cs,
                    ip_before
                );
                self.halted = true;
                return Ok(StepResult::Halt);
            }
        }

        if self.breakpoints.contains(&phys_ip_start) {
            log::info!("Breakpoint hit at {:04X}:{:08X}", cs, ip_before);
        }

        // Reset prefix for new instruction
        self.prefix = PrefixState::default();

        let mut opcode = self.fetch_u8(bus);
        loop {
            match opcode {
                // Segment overrides
                0x26 => self.prefix.seg_override = Some(SegReg::ES),
                0x2E => self.prefix.seg_override = Some(SegReg::CS),
                0x36 => self.prefix.seg_override = Some(SegReg::SS),
                0x3E => self.prefix.seg_override = Some(SegReg::DS),
                0x64 => self.prefix.seg_override = Some(SegReg::FS),
                0x65 => self.prefix.seg_override = Some(SegReg::GS),
                // Size overrides
                0x66 => self.prefix.operand_32 = !self.prefix.operand_32,
                0x67 => self.prefix.address_32 = !self.prefix.address_32,
                // LOCK/REP prefixes
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
            0xCF => {
                let new_ip = self.pop16(bus);
                let new_cs = self.pop16(bus);
                let new_flags = self.pop16(bus);
                self.regs.ip = new_ip as u32;
                self.regs.set_seg(SegReg::CS, new_cs);
                self.regs.flags = (self.regs.flags & !0xFFFF) | (new_flags as u32);
                Ok(StepResult::Continue)
            }
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

            0xF2 => {
                // REPNE
                let next = self.fetch_u8(bus);
                loop {
                    let cx = self.regs.get16(Reg16::CX);
                    if cx == 0 {
                        break;
                    }
                    self.regs.set16(Reg16::CX, cx.wrapping_sub(1));
                    match next {
                        0xAE => self.scasb(bus),
                        0xAF => self.scasw(bus),
                        _ => {
                            log::warn!("REPNE {:02X} unhandled", next);
                            break;
                        }
                    }
                    if self.regs.get_zf() {
                        break;
                    }
                }
                Ok(StepResult::Continue)
            }
            0xF3 => {
                // REP / REPE
                let next = self.fetch_u8(bus);
                loop {
                    let cx = self.regs.get16(Reg16::CX);
                    if cx == 0 {
                        break;
                    }
                    self.regs.set16(Reg16::CX, cx.wrapping_sub(1));
                    match next {
                        0xA4 => self.movsb(bus),
                        0xA5 => self.movsw(bus),
                        0xAA => self.stosb(bus),
                        0xAB => self.stosw(bus),
                        0xAC => self.lodsb(bus),
                        0xAD => self.lodsw(bus),
                        0xAE => {
                            self.scasb(bus);
                            if !self.regs.get_zf() {
                                break;
                            }
                        }
                        0xAF => {
                            self.scasw(bus);
                            if !self.regs.get_zf() {
                                break;
                            }
                        }
                        _ => {
                            log::warn!("REP {:02X} unhandled", next);
                            break;
                        }
                    }
                }
                Ok(StepResult::Continue)
            }

            0xE4..=0xE7 | 0xEC..=0xEF => {
                // I/O stubs
                match opcode {
                    0xE4 | 0xEC => self.regs.set8(Reg8::AL, 0),
                    0xE5 | 0xED => self.regs.set16(Reg16::AX, 0),
                    0xE6 | 0xE7 | 0xEE | 0xEF => {}
                    _ => {}
                }
                if opcode == 0xE4 || opcode == 0xE5 || opcode == 0xE6 || opcode == 0xE7 {
                    self.fetch_u8(bus);
                }
                Ok(StepResult::Continue)
            }

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
            0xD7 => {
                let bx = self.regs.get16(Reg16::BX);
                let al = self.regs.get8(Reg8::AL) as u16;
                let v = bus
                    .mem
                    .seg_read_u8(self.regs.get_seg(SegReg::DS), bx.wrapping_add(al) as u32);
                self.regs.set8(Reg8::AL, v);
                Ok(StepResult::Continue)
            }
            0x9F => {
                let f = (self.regs.flags & 0xFF) as u8;
                self.regs.set8(Reg8::AH, f);
                Ok(StepResult::Continue)
            }
            0x9E => {
                let ah = self.regs.get8(Reg8::AH) as u32;
                self.regs.flags = (self.regs.flags & !0xFF) | (ah & 0xD5) | 0x02;
                Ok(StepResult::Continue)
            }
            0x9B => Ok(StepResult::Continue), // WAIT
            0xD4 | 0xD5 => {
                self.fetch_u8(bus);
                Ok(StepResult::Continue)
            } // AAM/AAD stubs
            0x27 | 0x2F | 0x37 | 0x3F => Ok(StepResult::Continue), // BCD stubs

            0xD8..=0xDF => {
                // FPU stubs
                let mb = self.fetch_u8(bus);
                let modrm = ModRm::decode(mb);
                if modrm.mode != 3 {
                    self.decode_ea(&modrm, bus);
                }
                Ok(StepResult::Continue)
            }

            _ => {
                panic!(
                    "Unimplemented opcode {:02X} at {:04X}:{:04X}",
                    opcode,
                    self.regs.get_seg(SegReg::CS),
                    ip_before
                );
            }
        }
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}
