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
        
        let dump_len = data.len().min(32);
        let hex: Vec<String> = data[..dump_len].iter().map(|b| format!("{:02X}", b)).collect();
        log::info!("  -> First 32 bytes: {}", hex.join(" "));

        let is_mz = data.len() >= 2 && data[0] == b'M' && data[1] == b'Z';

        if is_mz {
            log::info!("  -> Detected MZ executable");
            crate::cpu::load_mz(bus, cpu, data)?;
        } else {
            // COM Loader: Loaded at 0x1000:0100
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
