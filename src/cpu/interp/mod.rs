use anyhow::Result;
use std::collections::HashSet;

use crate::bus::Bus;
use crate::cpu::regs::{Regs, SegReg};
use crate::memory::Memory;

pub mod alu;
pub mod extended;
pub mod flow;
pub mod misc;
pub mod modrm;
pub mod stack;
pub mod string;
pub mod transfer;

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
        self.regs.ip = self.regs.ip.wrapping_add(1) & 0xFFFF;
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
        self.dispatch_alu(opcode, bus, ip_before)
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

    // Flag update helpers

    pub fn update_add8(&mut self, a: u8, b: u8, r: u8) {
        self.regs.update_flags_u8(r);
        self.regs.set_cf((a as u16).wrapping_add(b as u16) > 0xFF);
        self.regs.set_af(((a & 0xF).wrapping_add(b & 0xF)) > 0xF);
        let sign_match = (a ^ b) & 0x80 == 0;
        let result_sign_diff = (a ^ r) & 0x80 != 0;
        self.regs.set_of(sign_match && result_sign_diff);
    }

    pub fn update_add16(&mut self, a: u16, b: u16, r: u16) {
        self.regs.update_flags_u16(r);
        self.regs.set_cf((a as u32).wrapping_add(b as u32) > 0xFFFF);
        self.regs.set_af(((a & 0xF).wrapping_add(b & 0xF)) > 0xF);
        let sign_match = (a ^ b) & 0x8000 == 0;
        let result_sign_diff = (a ^ r) & 0x8000 != 0;
        self.regs.set_of(sign_match && result_sign_diff);
    }

    pub fn update_sub8(&mut self, a: u8, b: u8, r: u8) {
        self.regs.update_flags_u8(r);
        self.regs.set_cf(a < b);
        self.regs.set_af((a & 0xF) < (b & 0xF));
        let sign_diff = (a ^ b) & 0x80 != 0;
        let result_sign_diff = (a ^ r) & 0x80 != 0;
        self.regs.set_of(sign_diff && result_sign_diff);
    }

    pub fn update_sub16(&mut self, a: u16, b: u16, r: u16) {
        self.regs.update_flags_u16(r);
        self.regs.set_cf(a < b);
        self.regs.set_af((a & 0xF) < (b & 0xF));
        let sign_diff = (a ^ b) & 0x8000 != 0;
        let result_sign_diff = (a ^ r) & 0x8000 != 0;
        self.regs.set_of(sign_diff && result_sign_diff);
    }

    pub fn update_add32(&mut self, a: u32, b: u32, r: u32) {
        self.regs.update_flags_u32(r);
        self.regs.set_cf(r < a);
        self.regs.set_of(((a ^ r) & (b ^ r) & 0x80000000) != 0);
    }

    pub fn update_sub32(&mut self, a: u32, b: u32, r: u32) {
        self.regs.update_flags_u32(r);
        self.regs.set_cf(a < b);
        self.regs.set_of(((a ^ b) & (a ^ r) & 0x80000000) != 0);
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}
