use std::collections::HashMap;
use anyhow::Result;
use crate::cpu::regs::{Reg16, Reg8, Regs, SegReg};
use crate::cpu::interp::StepResult;
use crate::bus::Bus;
use crate::memory::Memory;
use dynasmrt::{dynasm, DynasmApi, DynasmLabelApi, ExecutableBuffer};

/// Simple Intermediate Representation for 8086 instructions.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum IrOp {
    // Data movement
    MovRegImm(Reg16, u16),
    MovRegReg(Reg16, Reg16),
    MovMemImm8(u32, u8), // PhysAddr, Val
    
    // Arithmetic
    AddRegImm(Reg16, u16),
    SubRegImm(Reg16, u16),
    CmpRegImm(Reg16, u16),

    // Control flow
    JmpRelative(i16),
    
    // System
    Int(u8),
    OutImm8(u16, Reg8),
    
    // Fallback/Marker
    SyncIp(u16),
    Exit(StepResult),
}

#[allow(dead_code)]
pub struct JitBlock {
    pub code: ExecutableBuffer,
    pub start_phys: u32,
}

pub struct JitRuntime {
    pub cache: HashMap<u32, JitBlock>,
}

impl JitRuntime {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    pub fn execute(&mut self, bus: &mut Bus, regs: &mut Regs) -> Result<StepResult> {
        let phys_ip = Memory::phys(regs.get_seg(SegReg::CS), regs.ip);
        
        if let Some(block) = self.cache.get(&phys_ip) {
            return Ok(self.run_block(block, bus, regs));
        }

        let ir = self.translate_block(bus, regs, phys_ip)?;
        if ir.is_empty() { return Ok(StepResult::Continue); }

        let block = self.compile_block(ir, phys_ip)?;
        self.cache.insert(phys_ip, block);
        
        let block = self.cache.get(&phys_ip).unwrap();
        Ok(self.run_block(block, bus, regs))
    }

    fn translate_block(&self, bus: &Bus, regs: &Regs, start_phys: u32) -> Result<Vec<IrOp>> {
        let mut ops = Vec::new();
        let mut curr_ip = regs.ip;
        let cs = regs.get_seg(SegReg::CS);
        let mut addr = start_phys;

        for _ in 0..50 {
            let opcode = bus.mem.read_u8(addr);
            let instr_size;

            match opcode {
                // MOV reg, imm16
                0xB8..=0xBF => {
                    let reg = (opcode & 7) as u8;
                    let imm = bus.mem.read_u16(addr + 1);
                    ops.push(IrOp::MovRegImm(Self::decode_reg16(reg), imm));
                    instr_size = 3;
                }
                // MOV r/m8, imm8 (C6 /0)
                0xC6 => {
                    let modrm = bus.mem.read_u8(addr + 1);
                    if (modrm >> 6) == 0 && (modrm & 7) == 6 { // [disp16]
                        let disp = bus.mem.read_u16(addr + 2);
                        let imm = bus.mem.read_u8(addr + 4);
                        let phys = Memory::phys(regs.get_seg(SegReg::DS), disp as u32);
                        ops.push(IrOp::MovMemImm8(phys, imm));
                        instr_size = 5;
                    } else { break; }
                }
                // ADD AX, imm16
                0x05 => {
                    let imm = bus.mem.read_u16(addr + 1);
                    ops.push(IrOp::AddRegImm(Reg16::AX, imm));
                    instr_size = 3;
                }
                // OUT imm8, AL
                0xE6 => {
                    let port = bus.mem.read_u8(addr + 1) as u16;
                    ops.push(IrOp::OutImm8(port, Reg8::AL));
                    instr_size = 2;
                }
                // JMP rel8
                0xEB => {
                    let rel = bus.mem.read_u8(addr + 1) as i8 as i16;
                    ops.push(IrOp::JmpRelative(rel));
                    ops.push(IrOp::SyncIp((curr_ip as u16).wrapping_add(2).wrapping_add(rel as u16)));
                    ops.push(IrOp::Exit(StepResult::Continue));
                    break;
                }
                // INT imm8
                0xCD => {
                    let num = bus.mem.read_u8(addr + 1);
                    ops.push(IrOp::SyncIp((curr_ip + 2) as u16));
                    ops.push(IrOp::Exit(StepResult::Interrupt(num)));
                    break;
                }
                0xF4 => {
                    ops.push(IrOp::Exit(StepResult::Halt));
                    break;
                }
                _ => {
                    ops.push(IrOp::Exit(StepResult::Continue));
                    break; // Unknown
                }
            }

            curr_ip = curr_ip.wrapping_add(instr_size);
            addr = Memory::phys(cs, curr_ip);
            ops.push(IrOp::SyncIp(curr_ip as u16));
        }

        Ok(ops)
    }

    fn compile_block(&self, ops: Vec<IrOp>, _start_phys: u32) -> Result<JitBlock> {
        let mut azm = dynasmrt::x64::Assembler::new()?;
        
        // Context: RCX = &mut Bus, RDX = &mut Regs
        dynasm!(azm
            ; push rbx
            ; push r12
            ; push r13
            ; mov rbx, rcx // Bus
            ; mov r12, rdx // Regs
            // Get memory pointer (unsafe but box is stable)
            ; mov r13, [rbx] // Bus starts with Memory, Memory starts with Box ptr? 
            // Actually let's just use a helper function to avoid fragility.
        );

        for op in ops {
            match op {
                IrOp::MovRegImm(reg, imm) => {
                    let off = (reg as i32) * 2;
                    dynasm!(azm ; mov WORD [r12 + off], imm as i16);
                }
                IrOp::AddRegImm(reg, imm) => {
                    let off = (reg as i32) * 2;
                    dynasm!(azm ; add WORD [r12 + off], imm as i16);
                }
                IrOp::MovMemImm8(phys, val) => {
                    // This needs more care with VRAM. We should call Bus::mem_write_u8.
                    // For now, let's keep it simple and exit for memory writes to be safe.
                    dynasm!(azm 
                        ; mov rcx, rbx
                        ; mov rdx, QWORD phys as i64
                        ; mov r8, QWORD val as i64
                        ; mov rax, QWORD Bus::mem_write_u8 as *const fn(&mut Bus, u32, u8) as i64
                        ; call rax
                    );
                }
                IrOp::OutImm8(port, _reg) => {
                    // Call Bus::io_write_u8(port, val)
                    dynasm!(azm
                        ; mov rcx, rbx
                        ; mov edx, port as i32
                        ; movzx r8, BYTE [r12]
                        ; mov rax, QWORD Bus::io_write_u8 as *const fn(&mut Bus, u16, u8) as i64
                        ; call rax
                    );
                }
                IrOp::SyncIp(ip) => {
                    dynasm!(azm ; mov WORD [r12 + 24], ip as i16);
                }
                IrOp::Exit(res) => {
                    let r = match res {
                        StepResult::Continue => 0,
                        StepResult::Halt => 1,
                        StepResult::Interrupt(n) => 100 + n as u64,
                        _ => 0,
                    };
                    dynasm!(azm ; mov rax, QWORD r as i64 ; jmp >exit);
                }
                _ => {}
            }
        }

        dynasm!(azm
            ; exit:
            ; pop r13
            ; pop r12
            ; pop rbx
            ; ret
        );

        Ok(JitBlock { code: azm.finalize().unwrap(), start_phys: _start_phys })
    }

    fn run_block(&self, block: &JitBlock, bus: &mut Bus, regs: &mut Regs) -> StepResult {
        let entry: extern "C" fn(&mut Bus, &mut Regs) -> u64 = unsafe {
            std::mem::transmute(block.code.ptr(dynasmrt::AssemblyOffset(0)))
        };
        let res = entry(bus, regs);
        match res {
            0 => StepResult::Continue,
            1 => StepResult::Halt,
            n if n >= 100 => StepResult::Interrupt((n - 100) as u8),
            _ => StepResult::Continue,
        }
    }

    fn decode_reg16(val: u8) -> Reg16 {
        match val & 7 {
            0 => Reg16::AX, 1 => Reg16::CX, 2 => Reg16::DX, 3 => Reg16::BX,
            4 => Reg16::SP, 5 => Reg16::BP, 6 => Reg16::SI, 7 => Reg16::DI,
            _ => unreachable!(),
        }
    }
}
