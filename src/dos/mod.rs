use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use anyhow::Result;

pub struct DosRuntime {
    pub files: HashMap<u16, File>,
    pub next_handle: u16,
}

impl DosRuntime {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            next_handle: 5, // 0=stdin, 1=stdout, 2=stderr, 3=stdaux, 4=stdprn
        }
    }

    pub fn open(&mut self, path: &str) -> Result<u16> {
        let file = File::open(path)?;
        let handle = self.next_handle;
        self.files.insert(handle, file);
        self.next_handle += 1;
        Ok(handle)
    }

    pub fn read(&mut self, handle: u16, buf: &mut [u8]) -> Result<usize> {
        if let Some(file) = self.files.get_mut(&handle) {
            Ok(file.read(buf)?)
        } else {
            anyhow::bail!("Invalid handle")
        }
    }

    pub fn seek(&mut self, handle: u16, offset: i64, whence: u8) -> Result<u64> {
        if let Some(file) = self.files.get_mut(&handle) {
            let pos = match whence {
                0 => SeekFrom::Start(offset as u64),
                1 => SeekFrom::Current(offset),
                2 => SeekFrom::End(offset),
                _ => anyhow::bail!("Invalid whence"),
            };
            Ok(file.seek(pos)?)
        } else {
            anyhow::bail!("Invalid handle")
        }
    }

    #[allow(dead_code)]
    pub fn close(&mut self, handle: u16) {
        self.files.remove(&handle);
    }
}
