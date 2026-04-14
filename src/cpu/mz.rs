use crate::cpu::{Interpreter, Reg16, SegReg};
use crate::memory::Memory;
use crate::bus::Bus;
use anyhow::{bail, Result};

#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct MzHeader {
    pub magic: [u8; 2],            // 00: "MZ"
    pub bytes_on_last_page: u16,  // 02
    pub pages_in_file: u16,        // 04: 512-byte pages
    pub relocations: u16,          // 06
    pub header_paragraphs: u16,    // 08: 16-byte paragraphs
    pub min_extra_paragraphs: u16, // 0A
    pub max_extra_paragraphs: u16, // 0C
    pub initial_ss: u16,           // 0E
    pub initial_sp: u16,           // 10
    pub checksum: u16,             // 12
    pub initial_ip: u16,           // 14
    pub initial_cs: u16,           // 16
    pub reloc_table_offset: u16,  // 18
    pub overlay_number: u16,       // 1A
}

#[allow(dead_code)]
pub struct RelocEntry {
    pub offset: u16,
    pub segment: u16,
}

pub fn load_mz(bus: &mut Bus, cpu: &mut Interpreter, data: &[u8]) -> Result<()> {
    if data.len() < 28 || &data[0..2] != b"MZ" {
        bail!("Not a valid MZ executable");
    }

    let header: MzHeader = unsafe { std::ptr::read_unaligned(data.as_ptr() as *const MzHeader) };
    
    // Copy fields into local variables to avoid unaligned reference errors
    let initial_cs = header.initial_cs;
    let initial_ip = header.initial_ip;
    let initial_ss = header.initial_ss;
    let initial_sp = header.initial_sp;
    let relocations_count = header.relocations;
    let reloc_table_offset = header.reloc_table_offset;
    let header_paragraphs = header.header_paragraphs;
    let pages_in_file = header.pages_in_file;
    let bytes_on_last_page = header.bytes_on_last_page;

    let header_size = header_paragraphs as usize * 16;
    let file_size = (pages_in_file as usize * 512)
        .saturating_sub(if bytes_on_last_page > 0 { 512 - bytes_on_last_page as usize } else { 0 });
    
    let code_size = file_size.saturating_sub(header_size);
    let code_data = &data[header_size..file_size];

    // Find a segment to load the program into.
    // Conventional DOS programs often load at 0x1000:0000 or similar.
    // Let's use 0x1000 for now.
    let load_segment: u16 = 0x1000;
    let load_phys = Memory::phys(load_segment, 0);

    log::info!("Loading MZ: CS:IP={:04X}:{:04X} SS:SP={:04X}:{:04X} CodeSize={} LoadSeg={:04X}",
        initial_cs, initial_ip, initial_ss, initial_sp, code_size, load_segment);

    bus.mem.load_bytes(load_phys, code_data);
    cpu.valid_ranges.push(load_phys..(load_phys + code_size as u32));

    // Apply relocations
    let reloc_offset = reloc_table_offset as usize;
    for i in 0..relocations_count as usize {
        let entry_ptr = reloc_offset + (i * 4);
        if entry_ptr + 4 > data.len() {
            break;
        }
        let off = u16::from_le_bytes([data[entry_ptr], data[entry_ptr+1]]);
        let seg = u16::from_le_bytes([data[entry_ptr+2], data[entry_ptr+3]]);
        
        // The relocation points to a word in the loaded image that needs to be updated.
        // The value at (load_segment + seg):off should have load_segment added to it.
        let target_seg = load_segment.wrapping_add(seg);
        let current_val = bus.mem.seg_read_u16(target_seg, off as u32);
        let new_val = current_val.wrapping_add(load_segment);
        bus.mem.seg_write_u16(target_seg, off as u32, new_val);
    }

    // Set up registers
    cpu.regs.set_seg(SegReg::CS, load_segment.wrapping_add(initial_cs));
    cpu.regs.ip = initial_ip as u32;
    cpu.regs.set_seg(SegReg::SS, load_segment.wrapping_add(initial_ss));
    cpu.regs.set16(Reg16::SP, initial_sp);
    
    // DS/ES typically point to the PSP (Program Segment Prefix), which is 256 bytes before the load segment.
    // But we don't have a PEP yet. Let's just point them to the load segment for now.
    cpu.regs.set_seg(SegReg::DS, load_segment);
    cpu.regs.set_seg(SegReg::ES, load_segment);

    Ok(())
}
