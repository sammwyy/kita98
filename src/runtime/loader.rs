use anyhow::Result;
use crate::bus::Bus;
use crate::cpu::{Interpreter, SegReg};
use crate::runtime::Runtime;

impl Runtime {
    pub fn execute_binary(
        bus: &mut Bus,
        cpu: &mut Interpreter,
        name: &str,
        ext: &str,
        data: &[u8],
    ) -> Result<()> {
        log::info!("Executing binary: {}.{}", name, ext);
        if ext.to_uppercase() == "EXE" {
            crate::cpu::load_mz(bus, cpu, data)?;
        } else {
            // COM Loader: Loaded at 0x0100 in a new segment.
            let load_seg = 0x1000;
            bus.mem.load_bytes(crate::memory::Memory::phys(load_seg, 0x0100), data);
            cpu.regs.set_seg(SegReg::CS, load_seg);
            cpu.regs.ip = 0x0100;
            cpu.regs.set_seg(SegReg::DS, load_seg);
            cpu.regs.set_seg(SegReg::ES, load_seg);
            log::info!("Loaded COM at {:04X}:0100", load_seg);
        }
        Ok(())
    }
}
