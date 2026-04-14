/// Real-mode 1MB memory map.
///
/// Physical address = (segment << 4) + offset
///
/// Layout (relevant regions):
///   0x00000 – 0x003FF  IVT  (Interrupt Vector Table, 256 × 4 bytes)
///   0x00400 – 0x004FF  BDA  (BIOS Data Area)
///   0x07C00 – 0x07DFF  Boot sector (512 bytes loaded by us)
///   0x07E00 –          Free conventional memory
///   0xA0000 –          Video RAM (stub)
///   0xF0000 –          BIOS ROM (stub)
pub const MEM_SIZE: usize = 0x10_0000; // 1 MB

pub struct Memory {
    data: Box<[u8; MEM_SIZE]>,
    pub trace: bool,
}

impl Memory {
    pub fn new() -> Self {
        // Box::new([0u8; MEM_SIZE]) can overflow the stack because it may create
        // the array on the stack before moving it to the heap.
        let data = vec![0u8; MEM_SIZE].into_boxed_slice();
        let data = unsafe {
            let raw = Box::into_raw(data) as *mut [u8; MEM_SIZE];
            Box::from_raw(raw)
        };

        Self { data, trace: false }
    }

    // address helpers-

    #[inline(always)]
    pub fn phys(segment: u16, offset: u32) -> u32 {
        ((segment as u32) << 4).wrapping_add(offset) & 0xF_FFFF
    }

    // raw byte access-

    pub fn read_u8(&self, addr: u32) -> u8 {
        let a = (addr & 0xF_FFFF) as usize;
        if self.trace {
            log::trace!("MEM RD8  [{:05X}] = {:02X}", a, self.data[a]);
        }
        self.data[a]
    }

    pub fn write_u8(&mut self, addr: u32, val: u8) {
        let a = (addr & 0xF_FFFF) as usize;
        if self.trace {
            log::trace!("MEM WR8  [{:05X}] <- {:02X}", a, val);
        }
        self.data[a] = val;
    }

    // 16-bit little-endian -

    pub fn read_u16(&self, addr: u32) -> u16 {
        let lo = self.read_u8(addr) as u16;
        let hi = self.read_u8(addr.wrapping_add(1)) as u16;
        lo | (hi << 8)
    }

    pub fn write_u16(&mut self, addr: u32, val: u16) {
        self.write_u8(addr, val as u8);
        self.write_u8(addr.wrapping_add(1), (val >> 8) as u8);
    }

    // 32-bit little-endian -

    pub fn read_u32(&self, addr: u32) -> u32 {
        let lo = self.read_u16(addr) as u32;
        let hi = self.read_u16(addr.wrapping_add(2)) as u32;
        lo | (hi << 16)
    }

    #[allow(dead_code)]
    pub fn write_u32(&mut self, addr: u32, val: u32) {
        self.write_u16(addr, val as u16);
        self.write_u16(addr.wrapping_add(2), (val >> 16) as u16);
    }

    // segment:offset wrappers --

    pub fn seg_read_u8(&self, seg: u16, off: u32) -> u8 {
        self.read_u8(Self::phys(seg, off))
    }

    pub fn seg_write_u8(&mut self, seg: u16, off: u32, val: u8) {
        self.write_u8(Self::phys(seg, off), val);
    }

    pub fn seg_read_u16(&self, seg: u16, off: u32) -> u16 {
        self.read_u16(Self::phys(seg, off))
    }

    pub fn seg_write_u16(&mut self, seg: u16, off: u32, val: u16) {
        self.write_u16(Self::phys(seg, off), val);
    }

    #[allow(dead_code)]
    pub fn seg_read_u32(&self, seg: u16, off: u32) -> u32 {
        self.read_u32(Self::phys(seg, off))
    }

    #[allow(dead_code)]
    pub fn seg_write_u32(&mut self, seg: u16, off: u32, val: u32) {
        self.write_u32(Self::phys(seg, off), val);
    }

    // bulk load

    /// Copy a slice into physical memory starting at `addr`.
    pub fn load_bytes(&mut self, addr: u32, bytes: &[u8]) {
        for (i, &b) in bytes.iter().enumerate() {
            let a = ((addr as usize).wrapping_add(i)) & 0xF_FFFF;
            self.data[a] = b;
        }
    }

    /// Read a slice from physical memory.
    pub fn read_bytes(&self, addr: u32, len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| self.read_u8(addr.wrapping_add(i as u32)))
            .collect()
    }

    // IVT helpers (each entry = seg:off, 4 bytes little-endian)--

    pub fn set_ivt(&mut self, vector: u8, segment: u16, offset: u16) {
        let base = (vector as u32) * 4;
        self.write_u16(base, offset);
        self.write_u16(base + 2, segment);
    }

    pub fn get_ivt(&self, vector: u8) -> (u16, u16) {
        let base = (vector as u32) * 4;
        let offset = self.read_u16(base);
        let segment = self.read_u16(base + 2);
        (segment, offset)
    }
}

impl Default for Memory {
    fn default() -> Self {
        Self::new()
    }
}
