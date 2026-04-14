use crate::disk::Disk;
use anyhow::{bail, Result};
use log::{debug, info};

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct FatFileSystem {
    pub partition_lba: u32,
    pub bytes_per_sector: u32,
    pub sectors_per_cluster: u8,
    pub reserved_sectors: u16,
    pub num_fats: u8,
    pub root_dir_entries: u16,
    pub sectors_per_fat: u32,
    
    pub fat_lba: u32,
    pub root_dir_lba: u32,
    pub data_lba: u32,
    pub is_fat12: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub ext: String,
    pub attr: u8,
    pub first_cluster: u16,
    pub size: u32,
}

impl FatFileSystem {
    pub fn detect(disk: &Disk) -> Result<Self> {
        // Scan for BPB in first 2048 sectors (tolerant scan)
        for lba in 0..2048 {
            let sector = match disk.read_sectors(lba, 1) {
                Ok(s) => s,
                Err(_) => continue,
            };

            // BPB fields - avoid strict signatures like 0xEB 0x52
            let bps = u16::from_le_bytes([sector[0x0B], sector[0x0C]]) as u32;
            let spc = sector[0x0D];
            let res = u16::from_le_bytes([sector[0x0E], sector[0x0F]]);
            let num_fats = sector[0x10];
            let root_ent = u16::from_le_bytes([sector[0x11], sector[0x12]]);
            let spf = u16::from_le_bytes([sector[0x16], sector[0x17]]);
            
            // Heuristics for PC-98 / Variant FAT
            if bps != 512 && bps != 1024 && bps != 2048 { continue; }
            if spc == 0 || !spc.is_power_of_two() { continue; }
            if num_fats == 0 || num_fats > 2 { continue; }
            if root_ent == 0 { continue; }
            if spf == 0 { continue; }

            // Heuristic check: See if root directory area looks like directory entries
            let logical_fat_start = res as u32;
            let logical_root_start = logical_fat_start + (num_fats as u32 * spf as u32);
            
            let physical_multiplier = bps / 512;
            let root_lba_phys = lba + (logical_root_start * physical_multiplier);
            
            if let Ok(root_data) = disk.read_sectors(root_lba_phys, 1) {
                let attr = root_data[11];
                // Directory attributes are usually 0x00, 0x01, 0x02, 0x04, 0x10, 0x20
                // If it's something wild like 0xFF, it's probably not a directory.
                if attr > 0x3F && root_data[0] != 0 && root_data[0] != 0xE5 {
                    continue; 
                }
            } else {
                continue;
            }

            // Detect FAT12 vs FAT16
            let total_sectors_small = u16::from_le_bytes([sector[0x13], sector[0x14]]) as u32;
            let total_sectors_large = u32::from_le_bytes([sector[0x20], sector[0x21], sector[0x22], sector[0x23]]);
            let total_sectors = if total_sectors_small != 0 { total_sectors_small } else { total_sectors_large };
            
            let root_dir_sectors = ((root_ent as u32 * 32) + (bps as u32 - 1)) / bps as u32;
            let data_sectors = total_sectors.saturating_sub(logical_root_start + root_dir_sectors);
            let clusters = data_sectors / spc as u32;
            
            let is_fat12 = clusters < 4085;

            info!("Filesystem detected (non-standard FAT) at LBA {}", lba);
            debug!("  BPB: BPS={}, SPC={}, Res={}, FATs={}, RootEnt={}, SPF={}, FAT12={}", 
                bps, spc, res, num_fats, root_ent, spf, is_fat12);

            return Ok(Self {
                partition_lba: lba,
                bytes_per_sector: bps,
                sectors_per_cluster: spc,
                reserved_sectors: res,
                num_fats,
                root_dir_entries: root_ent,
                sectors_per_fat: spf as u32,
                
                fat_lba: lba + (logical_fat_start * physical_multiplier),
                root_dir_lba: root_lba_phys,
                data_lba: root_lba_phys + (root_dir_sectors * physical_multiplier),
                is_fat12,
            });
        }

        bail!("No FAT filesystem found on disk")
    }

    pub fn list_root_dir(&self, disk: &Disk) -> Result<Vec<DirEntry>> {
        let mut entries = Vec::new();
        let bytes_per_entry = 32;
        let root_dir_size = self.root_dir_entries as u32 * bytes_per_entry;
        let sectors_to_read = (root_dir_size + 511) / 512;
        
        let data = disk.read_sectors(self.root_dir_lba, sectors_to_read)?;

        for i in 0..self.root_dir_entries as usize {
            let offset = i * 32;
            if offset + 32 > data.len() { break; }
            
            let entry_raw = &data[offset..offset+32];
            
            if entry_raw[0] == 0x00 { break; } // End of directory
            if entry_raw[0] == 0xE5 { continue; } // Deleted
            
            let attr = entry_raw[11];
            if (attr & 0x08) != 0 { continue; } // Volume label
            
            let name = String::from_utf8_local(&entry_raw[0..8]);
            let ext = String::from_utf8_local(&entry_raw[8..11]);
            let first_cluster = u16::from_le_bytes([entry_raw[26], entry_raw[27]]);
            let size = u32::from_le_bytes([entry_raw[28], entry_raw[29], entry_raw[30], entry_raw[31]]);

            if name.is_empty() { continue; }

            entries.push(DirEntry {
                name,
                ext,
                attr,
                first_cluster,
                size,
            });
        }

        Ok(entries)
    }

    pub fn read_file(&self, disk: &Disk, entry: &DirEntry) -> Result<Vec<u8>> {
        let mut data = Vec::with_capacity(entry.size as usize);
        let mut cluster = entry.first_cluster;
        
        let sectors_per_cluster_phys = (self.sectors_per_cluster as u32 * self.bytes_per_sector) / 512;

        while cluster >= 2 && cluster < (if self.is_fat12 { 0x0FF0 } else { 0xFFF0 }) {
            let lba = self.data_lba + (cluster as u32 - 2) * sectors_per_cluster_phys;
            if let Ok(cluster_data) = disk.read_sectors(lba, sectors_per_cluster_phys) {
                data.extend_from_slice(&cluster_data);
            } else {
                break;
            }
            
            if data.len() >= entry.size as usize {
                data.truncate(entry.size as usize);
                break;
            }

            // Next cluster from FAT
            cluster = self.get_next_cluster(disk, cluster)?;
        }

        Ok(data)
    }

    fn get_next_cluster(&self, disk: &Disk, cluster: u16) -> Result<u16> {
        if !self.is_fat12 { // FAT16
            let fat_offset = (cluster as u32) * 2;
            let fat_sector_lba = self.fat_lba + (fat_offset / 512);
            let sector_offset = (fat_offset % 512) as usize;
            
            let sector = disk.read_sectors(fat_sector_lba, 1)?;
            Ok(u16::from_le_bytes([sector[sector_offset], sector[sector_offset+1]]))
        } else { // FAT12
            let fat_offset = (cluster as u32 * 3) / 2;
            let fat_sector_lba = self.fat_lba + (fat_offset / 512);
            let sector_offset = (fat_offset % 512) as usize;
            
            let sector = disk.read_sectors(fat_sector_lba, 2)?;
            let val = u16::from_le_bytes([sector[sector_offset], sector[sector_offset+1]]);
            
            if cluster % 2 == 0 {
                Ok(val & 0x0FFF)
            } else {
                Ok(val >> 4)
            }
        }
    }

    pub fn find_entrypoint(&self, disk: &Disk) -> Result<DirEntry> {
        let entries = self.list_root_dir(disk)?;
        
        info!("Files:");
        for e in &entries {
            info!(" - {} ({} bytes)", e.full_name(), e.size);
        }

        // Priority 1: GAME.BAT
        if let Some(e) = entries.iter().find(|e| e.full_name().to_uppercase() == "GAME.BAT") {
            info!("Found entrypoint: GAME.BAT");
            return Ok(e.clone());
        }

        // Priority 2: START.BAT
        if let Some(e) = entries.iter().find(|e| e.full_name().to_uppercase() == "START.BAT") {
            info!("Found entrypoint: START.BAT");
            return Ok(e.clone());
        }

        // Priority 3: AUTOEXEC.BAT
        if let Some(e) = entries.iter().find(|e| e.full_name().to_uppercase() == "AUTOEXEC.BAT") {
            info!("Found entrypoint: AUTOEXEC.BAT");
            return Ok(e.clone());
        }

        // Priority 4: Any other .BAT
        if let Some(e) = entries.iter().find(|e| e.ext.to_uppercase() == "BAT") {
            info!("Found entrypoint: {}", e.full_name());
            return Ok(e.clone());
        }

        // Fallback: Largest .EXE
        let mut exes: Vec<_> = entries.iter().filter(|e| e.ext.to_uppercase() == "EXE").collect();
        exes.sort_by_key(|e| e.size);
        if let Some(e) = exes.last() {
            info!("Fallback to largest EXE: {}", e.full_name());
            return Ok((*e).clone());
        }

        bail!("No suitable entrypoint found")
    }

    pub fn parse_bat(&self, content: &[u8]) -> Vec<String> {
        let text = String::from_utf8_lossy(content);
        text.lines()
            .map(|l| l.trim().to_uppercase())
            .filter(|l| !l.is_empty())
            .collect()
    }
}

trait Utf8Local {
    fn from_utf8_local(bytes: &[u8]) -> String;
}

impl Utf8Local for String {
    fn from_utf8_local(bytes: &[u8]) -> String {
        bytes.iter()
            .map(|&b| if b >= 32 && b <= 126 { b as char } else { ' ' })
            .collect::<String>()
            .trim()
            .to_string()
    }
}

impl DirEntry {
    pub fn full_name(&self) -> String {
        if self.ext.is_empty() {
            self.name.clone()
        } else {
            format!("{}.{}", self.name, self.ext)
        }
    }
}
