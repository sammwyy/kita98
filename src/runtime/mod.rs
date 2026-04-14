use anyhow::{bail, Context, Result};
use crate::bus::Bus;
use crate::cpu::{Interpreter, JitRuntime, SegReg, StepResult};
use crate::dos::DosRuntime;
use crate::memory::Memory;

pub mod interrupts;
pub mod loader;

pub const BOOT_SEG: u16 = 0x0000;
pub const BOOT_OFF: u16 = 0x7C00;
const STUB_BIOS_SEG: u16 = 0xF000;
const STUB_BIOS_OFF: u16 = 0x0100;

pub struct Runtime {
    pub cpu: Interpreter,
    pub bus: Bus,
    pub jit: JitRuntime,
    pub dos: DosRuntime,
    pub max_instructions: u64,
    pub batch_queue: Vec<crate::disk::fat::DirEntry>,
    pub current_batch_index: usize,
}

impl Runtime {
    pub fn new(
        file_path: &str,
        trace: bool,
        dump_every: u64,
        max_instructions: u64,
    ) -> Result<Self> {
        let is_exe = file_path.to_lowercase().ends_with(".exe");
        let mut final_batch_queue = Vec::new();

        let mut bus = if is_exe {
            Bus::new(None)
        } else {
            let disk = crate::disk::Disk::load(file_path)?;
            Bus::new(Some(disk))
        };

        // Initialize stub BIOS IVT
        let stub_phys = Memory::phys(STUB_BIOS_SEG, STUB_BIOS_OFF as u32);
        bus.mem.write_u8(stub_phys, 0xCF); // IRET

        for v in 0..=255 {
            bus.mem.set_ivt(v as u8, STUB_BIOS_SEG, STUB_BIOS_OFF);
        }

        let mut cpu = Interpreter::new();
        cpu.trace = trace;
        cpu.dump_every = dump_every;

        if is_exe {
            let data = std::fs::read(file_path)?;
            crate::cpu::load_mz(&mut bus, &mut cpu, &data)?;
        } else {
            let disk = bus.disk.as_ref().context("No disk loaded")?;
            match crate::disk::fat::FatFileSystem::detect(disk) {
                Ok(fs) => {
                    let entry = fs.find_entrypoint(disk)?;
                    if entry.ext.to_uppercase() == "BAT" {
                        let content = fs.read_file(disk, &entry)?;
                        let commands = fs.parse_bat(&content);
                        for cmd in commands {
                            let parts: Vec<&str> = cmd.split_whitespace().collect();
                            if let Some(&exe_name) = parts.get(0) {
                                let clean_name = if exe_name.starts_with('@') { &exe_name[1..] } else { exe_name };
                                if ["ECHO", "REM", "SET", "CLS", "PAUSE", "IF", "GOTO"].contains(&clean_name.to_uppercase().as_str()) {
                                    continue;
                                }
                                let targets = if clean_name.contains('.') {
                                    vec![clean_name.to_string()]
                                } else {
                                    vec![format!("{}.COM", clean_name), format!("{}.EXE", clean_name)]
                                };
                                if let Ok(root) = fs.list_root_dir(disk) {
                                    if let Some(target) = root.iter().find(|f| targets.contains(&f.full_name().to_uppercase())) {
                                        final_batch_queue.push(target.clone());
                                    }
                                }
                            }
                        }
                    } else {
                        final_batch_queue.push(entry);
                    }

                    if let Some(first) = final_batch_queue.get(0).cloned() {
                        let data = fs.read_file(disk, &first)?;
                        Self::execute_binary(&mut bus, &mut cpu, &first.name, &first.ext, &data)?;
                    } else {
                        bail!("No runnable files found on disk.");
                    }
                }
                Err(_) => {
                    // Fallback to boot sector
                    let boot_sector = disk.read_boot_sector()?;
                    bus.mem.load_bytes(Memory::phys(BOOT_SEG, BOOT_OFF as u32), &boot_sector);
                    cpu.regs.set_seg(SegReg::CS, BOOT_SEG);
                    cpu.regs.ip = BOOT_OFF as u32;
                    cpu.regs.set_seg(SegReg::SS, 0x0000);
                    cpu.regs.set16(crate::cpu::Reg16::SP, 0x7BFE);
                }
            }
            cpu.regs.set8(crate::cpu::Reg8::DL, 0x80);
        }

        Ok(Self {
            cpu,
            bus,
            jit: JitRuntime::new(),
            dos: DosRuntime::new(),
            max_instructions,
            batch_queue: final_batch_queue,
            current_batch_index: 0,
        })
    }

    pub fn step(&mut self) -> Result<StepResult> {
        if self.cpu.halted {
            return Ok(StepResult::Halt);
        }

        if self.max_instructions > 0 && self.cpu.instructions_executed >= self.max_instructions {
            log::info!("Instruction limit reached: {}", self.max_instructions);
            self.cpu.halted = true;
            return Ok(StepResult::Halt);
        }

        let result = self.jit.execute(&mut self.bus, &mut self.cpu.regs)?;
        let result = if result == StepResult::Continue {
            self.cpu.execute_one(&mut self.bus)?
        } else {
            result
        };

        self.handle_step_result(result)
    }

    fn handle_step_result(&mut self, result: StepResult) -> Result<StepResult> {
        match result {
            StepResult::Continue => Ok(StepResult::Continue),
            StepResult::Halt => {
                self.current_batch_index += 1;
                if self.current_batch_index < self.batch_queue.len() {
                    let disk = self.bus.disk.as_ref().context("No disk loaded")?;
                    let fs = crate::disk::fat::FatFileSystem::detect(disk)?;
                    let entry = self.batch_queue[self.current_batch_index].clone();
                    let data = fs.read_file(disk, &entry)?;
                    self.cpu.halted = false;
                    self.cpu.regs = crate::cpu::regs::Regs::new();
                    Self::execute_binary(&mut self.bus, &mut self.cpu, &entry.name, &entry.ext, &data)?;
                    return Ok(StepResult::Continue);
                }
                Ok(StepResult::Halt)
            }
            StepResult::Interrupt(num) => {
                let flags = self.cpu.regs.flags as u16;
                let cs = self.cpu.regs.get_seg(SegReg::CS);
                let ip = self.cpu.regs.ip as u16;
                self.cpu.push16(&mut self.bus, flags);
                self.cpu.push16(&mut self.bus, cs);
                self.cpu.push16(&mut self.bus, ip);

                self.cpu.regs.set_flag(crate::cpu::regs::flags::IF, false);
                self.cpu.regs.set_flag(crate::cpu::regs::flags::TF, false);

                if !self.handle_interrupt(num) {
                    let (iseg, ioff) = self.bus.mem.get_ivt(num);
                    self.cpu.regs.set_seg(SegReg::CS, iseg);
                    self.cpu.regs.ip = ioff as u32;
                } else {
                    let rip = self.cpu.pop16(&mut self.bus);
                    let rcs = self.cpu.pop16(&mut self.bus);
                    let rflgs = self.cpu.pop16(&mut self.bus);
                    self.cpu.regs.ip = rip as u32;
                    self.cpu.regs.set_seg(SegReg::CS, rcs);
                    self.cpu.regs.flags = (self.cpu.regs.flags & !0xFFFF) | (rflgs as u32);
                }
                Ok(StepResult::Continue)
            }
            StepResult::Unimplemented(op) => {
                log::error!("Unimplemented opcode {:02X} at {:04X}:{:04X}", op, self.cpu.regs.get_seg(SegReg::CS), self.cpu.regs.ip);
                Ok(StepResult::Unimplemented(op))
            }
        }
    }

    #[allow(dead_code)]
    pub fn run(&mut self) -> Result<()> {
        log::info!("Starting execution loop...");
        loop {
            if self.step()? == StepResult::Halt {
                break;
            }
        }
        log::info!("Execution finished. Total instructions: {}", self.cpu.instructions_executed);
        Ok(())
    }
}
