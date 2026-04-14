/// Disk subsystem – loads a `.hdi` image and exposes sector-level reads.
///
/// HDI format (NEC PC-98 virtual disk):
///   Bytes 0x000 – 0x00F : signature / header  (we detect and skip it)
///   Bytes 0x010 – ...   : raw sector data
///
/// For simplicity we support two layouts:
///   1. Raw image  – no header, sector 0 starts at file offset 0.
///   2. HDI header – 4096-byte (0x1000) header followed by raw sectors.
///
/// We auto-detect by checking for the magic "HDI\x00" or "T98HDDIMAGE.R0"
/// at offset 0.  Everything else is treated as raw.
///
/// Sector size is assumed to be 512 bytes (standard).
use std::fs;
use std::path::Path;

pub mod fat;

use anyhow::{bail, Context, Result};

pub const SECTOR_SIZE: usize = 512;

/// Known HDI header sizes (bytes).
const HDR_SIZE_STANDARD: usize = 4096; // most common
const HDR_SIZE_T98: usize = 0x110; // T98HDDIMAGE variant

#[allow(dead_code)]
pub struct Disk {
    pub data: Vec<u8>,
    pub header_offset: usize,
    pub sector_size: usize,
    pub total_sectors: usize,
    pub cylinders: u16,
    pub heads: u8,
    pub sectors_per_track: u8,
    pub image_path: String,
}

impl Disk {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_str = path.as_ref().display().to_string();
        let data =
            fs::read(&path).with_context(|| format!("Failed to open disk image: {}", path_str))?;

        if data.is_empty() {
            bail!("Disk image is empty: {}", path_str);
        }

        let (header_offset, cylinders, heads, spt) = Self::detect_geometry(&data);

        let effective_size = data.len().saturating_sub(header_offset);
        let sector_size = SECTOR_SIZE;
        let total_sectors = effective_size / sector_size;

        log::info!(
            "Disk image loaded: {} bytes, header_offset={}, {} sectors ({}C/{}H/{}S)",
            data.len(),
            header_offset,
            total_sectors,
            cylinders,
            heads,
            spt,
        );

        Ok(Self {
            data,
            header_offset,
            sector_size,
            total_sectors,
            cylinders,
            heads,
            sectors_per_track: spt,
            image_path: path_str,
        })
    }

    // geometry detection-

    fn detect_geometry(data: &[u8]) -> (usize, u16, u8, u8) {
        // T98HDDIMAGE.R0 magic
        if data.len() >= 16 && &data[0..15] == b"T98HDDIMAGE.R0\x00" {
            // Header layout (little-endian):
            //   0x000  magic (15 bytes + \0)
            //   0x010  sector_size (u16)
            //   0x012  sectors_per_track (u16)
            //   0x014  heads (u16)
            //   0x016  cylinders (u16)
            let spt = u16::from_le_bytes([data[0x12], data[0x13]]) as u8;
            let heads = u16::from_le_bytes([data[0x14], data[0x15]]) as u8;
            let cyls = u16::from_le_bytes([data[0x16], data[0x17]]);
            log::info!("Detected T98HDDIMAGE header");
            return (HDR_SIZE_T98, cyls, heads, spt);
        }

        // Standard HDI magic "HDI\x00"
        if data.len() >= 4 && &data[0..4] == b"HDI\x00" {
            // Geometry at fixed offsets (common NEC HDI variant):
            //   0x008  disk_type u16
            //   0x00A  header_size u32
            //   0x00E  sector_size u32
            //   0x012  sectors_per_track u32
            //   0x016  heads u32
            //   0x01A  cylinders u32
            let hdr_sz = if data.len() >= 14 {
                u32::from_le_bytes([data[0x0A], data[0x0B], data[0x0C], data[0x0D]]) as usize
            } else {
                HDR_SIZE_STANDARD
            };
            let spt = if data.len() >= 0x16 {
                u32::from_le_bytes([data[0x12], data[0x13], data[0x14], data[0x15]]) as u8
            } else {
                17
            };
            let heads = if data.len() >= 0x1A {
                u32::from_le_bytes([data[0x16], data[0x17], data[0x18], data[0x19]]) as u8
            } else {
                8
            };
            let cyls = if data.len() >= 0x1E {
                u32::from_le_bytes([data[0x1A], data[0x1B], data[0x1C], data[0x1D]]) as u16
            } else {
                615
            };
            log::info!("Detected standard HDI header (size={})", hdr_sz);
            return (hdr_sz, cyls, heads, spt);
        }

        // Raw image – assume common PC geometry
        log::info!("No HDI header detected – treating as raw disk image");
        (0, 615, 8, 17) // common PC-98 geometry
    }

    // LBA / CHS

    /// Convert CHS to LBA (zero-based).
    pub fn chs_to_lba(&self, cylinder: u16, head: u8, sector: u8) -> u32 {
        // sector in CHS is 1-based
        let spt = self.sectors_per_track as u32;
        let heads = self.heads as u32;
        (cylinder as u32) * heads * spt + (head as u32) * spt + (sector as u32).saturating_sub(1)
    }

    // sector I/O

    /// Read `count` sectors starting at LBA `lba` into a Vec<u8>.
    pub fn read_sectors(&self, lba: u32, count: u32) -> Result<Vec<u8>> {
        let start = self.header_offset + (lba as usize) * self.sector_size;
        let len = (count as usize) * self.sector_size;
        let end = start + len;

        if end > self.data.len() {
            bail!(
                "Disk read out of range: LBA {} count {} (image size {})",
                lba,
                count,
                self.data.len()
            );
        }

        Ok(self.data[start..end].to_vec())
    }

    /// Read the raw boot sector (LBA 0).
    pub fn read_boot_sector(&self) -> Result<[u8; SECTOR_SIZE]> {
        let bytes = self.read_sectors(0, 1)?;
        let mut arr = [0u8; SECTOR_SIZE];
        arr.copy_from_slice(&bytes);
        Ok(arr)
    }
}
