/// Minimal device stubs.
///
/// Each device implements `Device` which lets it respond to I/O port reads/writes
/// and optionally handle memory-mapped ranges.

// Keyboard stub

pub struct KeyboardDevice {
    #[allow(dead_code)]
    pub pending: std::collections::VecDeque<u8>,
}

impl KeyboardDevice {
    pub fn new() -> Self {
        Self {
            pending: std::collections::VecDeque::new(),
        }
    }

    /// INT 16h handler stub.  Returns (AH=scan_code, AL=char) or stall.
    #[allow(dead_code)]
    pub fn handle_int16(&mut self, ah: u8) -> Option<(u8, u8)> {
        match ah {
            0x00 => {
                // Read char – block until available.
                // We stub: return nothing (no key).
                None
            }
            0x01 => {
                // Check if key available
                None // ZF will be set by caller to indicate no key
            }
            _ => {
                log::debug!("INT 16h/{:02X}: unhandled", ah);
                None
            }
        }
    }
}

impl Default for KeyboardDevice {
    fn default() -> Self {
        Self::new()
    }
}

// Timer stub

pub struct TimerDevice {
    pub ticks: u64,
}

impl TimerDevice {
    pub fn new() -> Self {
        Self { ticks: 0 }
    }
    #[allow(dead_code)]
    pub fn tick(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);
    }
}

impl Default for TimerDevice {
    fn default() -> Self {
        Self::new()
    }
}
