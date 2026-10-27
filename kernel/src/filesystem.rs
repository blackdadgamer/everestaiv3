//! Legacy ATA PIO block reads and a small read-only FAT32 root-directory reader.
//!
//! This deliberately supports the QEMU test disk's primary-master IDE device and
//! 512-byte sectors. It is not a general AHCI/SATA driver.
use core::arch::asm;

pub const SECTOR_SIZE: usize = 512;
const ATA_DATA: u16 = 0x1f0;
const ATA_SECTOR_COUNT: u16 = 0x1f2;
const ATA_LBA0: u16 = 0x1f3;
const ATA_LBA1: u16 = 0x1f4;
const ATA_LBA2: u16 = 0x1f5;
const ATA_DRIVE: u16 = 0x1f6;
const ATA_STATUS: u16 = 0x1f7;
const ATA_COMMAND: u16 = 0x1f7;
const CMD_READ_SECTORS: u8 = 0x20;
const STATUS_BSY: u8 = 0x80;
const STATUS_DRQ: u8 = 0x08;
const STATUS_ERR: u8 = 0x01;
const POLL_LIMIT: usize = 1_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FsError {
    DeviceTimeout,
    DeviceError,
    InvalidBootSector,
    NotFat32,
    UnsupportedSectorSize,
    InvalidCluster,
    FileNotFound,
    FileTooLarge,
    CorruptChain,
}

/// Read one 512-byte LBA sector from the primary IDE master using ATA PIO.
pub fn read_sector(lba: u32, output: &mut [u8; SECTOR_SIZE]) -> Result<(), FsError> {
    unsafe {
        // Wait for the bus to become idle before selecting the drive.
        if !wait_not_busy()? { return Err(FsError::DeviceTimeout); }
        outb(ATA_DRIVE, 0xe0 | ((lba >> 24) as u8 & 0x0f));
        // Four alternate-status reads provide the required selection delay.
        let _ = inb(0x3f6); let _ = inb(0x3f6); let _ = inb(0x3f6); let _ = inb(0x3f6);
        outb(ATA_SECTOR_COUNT, 1);
        outb(ATA_LBA0, lba as u8);
        outb(ATA_LBA1, (lba >> 8) as u8);
        outb(ATA_LBA2, (lba >> 16) as u8);
        outb(ATA_COMMAND, CMD_READ_SECTORS);
        if !wait_data_ready()? { return Err(FsError::DeviceTimeout); }
        for word in 0..(SECTOR_SIZE / 2) {
            let value = inw(ATA_DATA);
            output[word * 2] = value as u8;
            output[word * 2 + 1] = (value >> 8) as u8;
        }
    }
    Ok(())
}

unsafe fn wait_not_busy() -> Result<bool, FsError> {
    for _ in 0..POLL_LIMIT {
        let status = unsafe { inb(ATA_STATUS) };
        if status == 0 { return Err(FsError::DeviceError); }
        if status & STATUS_BSY == 0 { return Ok(true); }
    }
    Ok(false)
}
unsafe fn wait_data_ready() -> Result<bool, FsError> {
    for _ in 0..POLL_LIMIT {
        let status = unsafe { inb(ATA_STATUS) };
        if status == 0 { return Err(FsError::DeviceError); }
        if status & STATUS_ERR != 0 { return Err(FsError::DeviceError); }
        if status & STATUS_BSY == 0 && status & STATUS_DRQ != 0 { return Ok(true); }
    }
    Ok(false)
}
unsafe fn outb(port: u16, value: u8) {
    unsafe { asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags)); }
}
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe { asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags)); }
    value
}
unsafe fn inw(port: u16) -> u16 {
    let value: u16;
    unsafe { asm!("in ax, dx", in("dx") port, out("ax") value, options(nomem, nostack, preserves_flags)); }
    value
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Fat32Volume {
    pub partition_lba: u32,
    pub sectors_per_cluster: u8,
    pub reserved_sectors: u16,
    pub fat_count: u8,
    pub sectors_per_fat: u32,
    pub root_cluster: u32,
    pub total_sectors: u32,
}
impl Fat32Volume {
    pub fn parse_boot_sector(lba: u32, sector: &[u8; SECTOR_SIZE]) -> Result<Self, FsError> {
        if sector[510] != 0x55 || sector[511] != 0xaa { return Err(FsError::InvalidBootSector); }
        if le16(sector, 11) != 512 { return Err(FsError::UnsupportedSectorSize); }
        let spc = sector[13];
        let reserved = le16(sector, 14);
        let fats = sector[16];
        let root_entries = le16(sector, 17);
        let total16 = le16(sector, 19) as u32;
        let fat16 = le16(sector, 22) as u32;
        let total = if total16 != 0 { total16 } else { le32(sector, 32) };
        let fat_size = if fat16 != 0 { fat16 } else { le32(sector, 36) };
        let root_cluster = le32(sector, 44) & 0x0fff_ffff;
        if spc == 0 || !spc.is_power_of_two() || reserved == 0 || fats == 0 || total == 0 {
            return Err(FsError::InvalidBootSector);
        }
        // FAT32 uses a zero FAT16-size field and zero fixed-root-entry count.
        if fat16 != 0 || root_entries != 0 || fat_size == 0 || root_cluster < 2 {
            return Err(FsError::NotFat32);
        }
        let first_data = reserved as u32 + (fats as u32).saturating_mul(fat_size);
        if first_data >= total { return Err(FsError::InvalidBootSector); }
        Ok(Self { partition_lba: lba, sectors_per_cluster: spc,
            reserved_sectors: reserved, fat_count: fats, sectors_per_fat: fat_size,
            root_cluster, total_sectors: total })
    }
    fn first_data_sector(&self) -> u32 {
        self.partition_lba + self.reserved_sectors as u32
            + self.fat_count as u32 * self.sectors_per_fat
    }
    fn cluster_sector(&self, cluster: u32) -> Result<u32, FsError> {
        if cluster < 2 { return Err(FsError::InvalidCluster); }
        let offset = (cluster - 2).checked_mul(self.sectors_per_cluster as u32).ok_or(FsError::InvalidCluster)?;
        let lba = self.first_data_sector().checked_add(offset).ok_or(FsError::InvalidCluster)?;
        if lba >= self.partition_lba.saturating_add(self.total_sectors) { return Err(FsError::InvalidCluster); }
        Ok(lba)
    }
    fn next_cluster(&self, cluster: u32) -> Result<Option<u32>, FsError> {
        let fat_offset = cluster.checked_mul(4).ok_or(FsError::CorruptChain)?;
        let fat_sector = self.partition_lba + self.reserved_sectors as u32 + fat_offset / 512;
        let mut sector = [0u8; 512];
        read_sector(fat_sector, &mut sector)?;
        let next = le32(&sector, (fat_offset % 512) as usize) & 0x0fff_ffff;
        if next >= 0x0fff_fff8 { Ok(None) }
        else if next < 2 || next == 0x0fff_fff7 { Err(FsError::CorruptChain) }
        else { Ok(Some(next)) }
    }
    /// List visible short-name files in the root directory into caller-owned bounded storage.
    /// Names are the raw 11-byte FAT 8.3 form; long-file-name entries are skipped.
    pub fn list_root_files(&self, output: &mut [[u8; 11]], count: &mut usize) -> Result<(), FsError> {
        *count = 0;
        let mut cluster = self.root_cluster;
        let max_clusters = self.total_sectors / self.sectors_per_cluster as u32 + 1;
        for _ in 0..max_clusters {
            let first = self.cluster_sector(cluster)?;
            for sector_index in 0..self.sectors_per_cluster as u32 {
                let mut sector = [0u8; 512];
                read_sector(first + sector_index, &mut sector)?;
                for entry in sector.chunks_exact(32) {
                    if entry[0] == 0 { return Ok(()); }
                    if entry[0] == 0xe5 || entry[11] == 0x0f || entry[11] & 0x08 != 0 { continue; }
                    if *count < output.len() {
                        output[*count].copy_from_slice(&entry[..11]);
                        *count += 1;
                    }
                }
            }
            match self.next_cluster(cluster)? {
                Some(next) => cluster = next,
                None => return Ok(()),
            }
        }
        Err(FsError::CorruptChain)
    }
    /// Scan root directory entries and return the cluster and size of an 8.3 filename.
    pub fn find_root_file(&self, wanted: &[u8; 11]) -> Result<(u32, u32), FsError> {
        let mut cluster = self.root_cluster;
        let max_clusters = self.total_sectors / self.sectors_per_cluster as u32 + 1;
        for _ in 0..max_clusters {
            let first = self.cluster_sector(cluster)?;
            for sector_index in 0..self.sectors_per_cluster as u32 {
                let mut sector = [0u8; 512];
                read_sector(first + sector_index, &mut sector)?;
                for entry in sector.chunks_exact(32) {
                    if entry[0] == 0 { return Err(FsError::FileNotFound); }
                    if entry[0] == 0xe5 || entry[11] == 0x0f || entry[11] & 0x08 != 0 { continue; }
                    if &entry[..11] == wanted {
                        let high = le16(entry, 20) as u32;
                        let low = le16(entry, 26) as u32;
                        let file_cluster = (high << 16) | low;
                        if file_cluster < 2 && le32(entry, 28) != 0 { return Err(FsError::InvalidCluster); }
                        return Ok((file_cluster, le32(entry, 28)));
                    }
                }
            }
            match self.next_cluster(cluster)? {
                Some(next) => cluster = next,
                None => return Err(FsError::FileNotFound),
            }
        }
        Err(FsError::CorruptChain)
    }
    /// Copy a file's contents into caller-provided storage, bounded by both file size and output.
    pub fn read_file(&self, first_cluster: u32, file_size: u32, output: &mut [u8]) -> Result<usize, FsError> {
        let length = file_size as usize;
        if length > output.len() { return Err(FsError::FileTooLarge); }
        if length == 0 { return Ok(0); }
        let mut copied = 0usize;
        let mut cluster = first_cluster;
        let max_clusters = self.total_sectors / self.sectors_per_cluster as u32 + 1;
        for _ in 0..max_clusters {
            let first = self.cluster_sector(cluster)?;
            for sector_index in 0..self.sectors_per_cluster as u32 {
                if copied >= length { return Ok(copied); }
                let mut sector = [0u8; 512];
                read_sector(first + sector_index, &mut sector)?;
                let amount = (length - copied).min(512);
                output[copied..copied + amount].copy_from_slice(&sector[..amount]);
                copied += amount;
            }
            if copied >= length { return Ok(copied); }
            match self.next_cluster(cluster)? {
                Some(next) => cluster = next,
                None => return Err(FsError::CorruptChain),
            }
        }
        Err(FsError::CorruptChain)
    }
}
fn le16(bytes: &[u8], offset: usize) -> u16 { u16::from_le_bytes([bytes[offset], bytes[offset + 1]]) }
fn le32(bytes: &[u8], offset: usize) -> u32 { u32::from_le_bytes([bytes[offset], bytes[offset + 1], bytes[offset + 2], bytes[offset + 3]]) }

/// Probe sector zero as a FAT32 volume, then scan a small MBR partition table if needed.
pub fn open_volume() -> Result<Fat32Volume, FsError> {
    let mut sector = [0u8; 512];
    read_sector(0, &mut sector)?;
    if let Ok(volume) = Fat32Volume::parse_boot_sector(0, &sector) { return Ok(volume); }
    if sector[510] != 0x55 || sector[511] != 0xaa { return Err(FsError::InvalidBootSector); }
    for index in 0..4 {
        let offset = 446 + index * 16;
        let start = le32(&sector, offset + 8);
        if start == 0 { continue; }
        read_sector(start, &mut sector)?;
        if let Ok(volume) = Fat32Volume::parse_boot_sector(start, &sector) { return Ok(volume); }
    }
    Err(FsError::NotFat32)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_fat32_boot_sector_fields() {
        let mut b = [0u8; 512];
        b[11..13].copy_from_slice(&512u16.to_le_bytes());
        b[13] = 1;
        b[14..16].copy_from_slice(&32u16.to_le_bytes());
        b[16] = 2;
        b[17..19].copy_from_slice(&0u16.to_le_bytes());
        b[19..21].copy_from_slice(&0u16.to_le_bytes());
        b[22..24].copy_from_slice(&0u16.to_le_bytes());
        b[32..36].copy_from_slice(&65536u32.to_le_bytes());
        b[36..40].copy_from_slice(&512u32.to_le_bytes());
        b[44..48].copy_from_slice(&2u32.to_le_bytes());
        b[510] = 0x55; b[511] = 0xaa;
        let volume = Fat32Volume::parse_boot_sector(0, &b).unwrap();
        assert_eq!(volume.root_cluster, 2);
        assert_eq!(volume.sectors_per_fat, 512);
        assert_eq!(volume.first_data_sector(), 1056);
    }
    #[test]
    fn rejects_non_fat32_boot_sector() {
        let mut b = [0u8; 512];
        b[510] = 0x55; b[511] = 0xaa;
        assert_eq!(Fat32Volume::parse_boot_sector(0, &b), Err(FsError::UnsupportedSectorSize));
    }
}
