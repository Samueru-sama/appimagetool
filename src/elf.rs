use std::path::Path;

use crate::error::Result;

/// Parsed ELF header info needed for section lookups.
struct ElfInfo {
    is_64bit: bool,
    is_le: bool,
    sh_off: u64,
    sh_entsize: u64,
    sh_num: u64,
    sh_strndx: u64,
}

impl ElfInfo {
    fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 64 || &data[0..4] != b"\x7fELF" {
            return None;
        }
        let is_64bit = data[4] == 2;
        let is_le = data[5] == 1;

        let (sh_off, sh_entsize, sh_num, sh_strndx) = if is_64bit {
            let sh_off = r64(data, 40, is_le);
            let sh_entsize = r16(data, 58, is_le) as u64;
            let sh_num = r16(data, 60, is_le) as u64;
            let sh_strndx = r16(data, 62, is_le) as u64;
            (sh_off, sh_entsize, sh_num, sh_strndx)
        } else {
            if data.len() < 52 {
                return None;
            }
            let sh_off = r32(data, 32, is_le) as u64;
            let sh_entsize = r16(data, 46, is_le) as u64;
            let sh_num = r16(data, 48, is_le) as u64;
            let sh_strndx = r16(data, 50, is_le) as u64;
            (sh_off, sh_entsize, sh_num, sh_strndx)
        };

        if sh_off == 0 || sh_entsize == 0 || sh_num == 0 {
            return None;
        }
        Some(ElfInfo {
            is_64bit,
            is_le,
            sh_off,
            sh_entsize,
            sh_num,
            sh_strndx,
        })
    }

    /// Get the strtab (section name string table) bytes.
    fn strtab<'a>(&self, data: &'a [u8]) -> Option<&'a [u8]> {
        let hdr_off = (self.sh_off + self.sh_strndx * self.sh_entsize) as usize;
        let (off, size) = if self.is_64bit {
            (
                r64(data, hdr_off + 24, self.is_le) as usize,
                r64(data, hdr_off + 32, self.is_le) as usize,
            )
        } else {
            (
                r32(data, hdr_off + 16, self.is_le) as usize,
                r32(data, hdr_off + 20, self.is_le) as usize,
            )
        };
        data.get(off..off + size)
    }

    /// Find section header index whose name matches.
    fn find_section_idx(&self, data: &[u8], name: &[u8]) -> Option<u64> {
        let strtab = self.strtab(data)?;
        for i in 0..self.sh_num {
            let hdr_off = (self.sh_off + i * self.sh_entsize) as usize;
            let name_idx = r32(data, hdr_off, self.is_le) as usize;
            let name_start = name_idx;
            let name_end = strtab[name_start..]
                .iter()
                .position(|&b| b == 0)
                .map(|p| name_start + p)
                .unwrap_or(strtab.len());
            if &strtab[name_start..name_end] == name {
                return Some(i);
            }
        }
        None
    }

    /// Get (offset, size) of a section by index.
    fn section_offset_size(&self, data: &[u8], idx: u64) -> (usize, usize) {
        let hdr_off = (self.sh_off + idx * self.sh_entsize) as usize;
        if self.is_64bit {
            (
                r64(data, hdr_off + 24, self.is_le) as usize,
                r64(data, hdr_off + 32, self.is_le) as usize,
            )
        } else {
            (
                r32(data, hdr_off + 16, self.is_le) as usize,
                r32(data, hdr_off + 20, self.is_le) as usize,
            )
        }
    }
}

/// Read the value of an ELF section by name from a binary.
pub fn read_section<'a>(data: &'a [u8], name: &str) -> Option<&'a [u8]> {
    let info = ElfInfo::parse(data)?;
    let idx = info.find_section_idx(data, name.as_bytes())?;
    let (off, size) = info.section_offset_size(data, idx);
    data.get(off..off + size)
}

/// Find the offset and size of an ELF section by name.
pub fn find_section(data: &[u8], name: &str) -> Option<(usize, usize)> {
    let info = ElfInfo::parse(data)?;
    let idx = info.find_section_idx(data, name.as_bytes())?;
    Some(info.section_offset_size(data, idx))
}

/// Write data into an ELF section by name. The data is null-padded to fill the section.
pub fn write_section(data: &mut [u8], name: &str, value: &[u8]) -> Result<()> {
    let info = ElfInfo::parse(data).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "not a valid ELF file")
    })?;

    let idx = info
        .find_section_idx(data, name.as_bytes())
        .ok_or_else(|| crate::error::Error::SectionNotFound(name.to_string()))?;

    let (sec_offset, sec_size) = info.section_offset_size(data, idx);

    if value.len() > sec_size {
        return Err(crate::error::Error::SectionOverflow {
            name: name.to_string(),
            size: value.len(),
            capacity: sec_size,
        });
    }

    data[sec_offset..sec_offset + sec_size].fill(0);
    data[sec_offset..sec_offset + value.len()].copy_from_slice(value);
    Ok(())
}

/// Write data into an ELF section in a file on disk.
pub fn write_section_file(path: &Path, name: &str, value: &[u8]) -> Result<()> {
    let mut data = std::fs::read(path)?;
    write_section(&mut data, name, value)?;
    std::fs::write(path, data)?;
    Ok(())
}

/// Replace a string pattern inside an ELF section (for patching runtime config).
pub fn patch_section_string(
    data: &mut [u8],
    section_name: &str,
    pattern: &str,
    replacement: &str,
) -> Result<()> {
    let (offset, size) = find_section(data, section_name)
        .ok_or_else(|| crate::error::Error::SectionNotFound(section_name.to_string()))?;

    let section = &data[offset..offset + size];
    let section_str = String::from_utf8_lossy(section);
    if let Some(pos) = section_str.find(pattern) {
        let byte_pos = offset + pos;
        let pat_len = pattern.len();
        let rep_len = replacement.len().min(pat_len);
        data[byte_pos..byte_pos + rep_len].copy_from_slice(&replacement.as_bytes()[..rep_len]);
        if rep_len < pat_len {
            data[byte_pos + rep_len..byte_pos + pat_len].fill(0);
        }
    }
    Ok(())
}

// Helpers for reading multi-byte values at a given offset.
fn r16(data: &[u8], off: usize, le: bool) -> u16 {
    let b = [data[off], data[off + 1]];
    if le {
        u16::from_le_bytes(b)
    } else {
        u16::from_be_bytes(b)
    }
}

fn r32(data: &[u8], off: usize, le: bool) -> u32 {
    let b = [data[off], data[off + 1], data[off + 2], data[off + 3]];
    if le {
        u32::from_le_bytes(b)
    } else {
        u32::from_be_bytes(b)
    }
}

fn r64(data: &[u8], off: usize, le: bool) -> u64 {
    let b = [
        data[off],
        data[off + 1],
        data[off + 2],
        data[off + 3],
        data[off + 4],
        data[off + 5],
        data[off + 6],
        data[off + 7],
    ];
    if le {
        u64::from_le_bytes(b)
    } else {
        u64::from_be_bytes(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_section() {
        let mut elf = create_test_elf(b".test", b"initial_data");

        let section = read_section(&elf, ".test").unwrap();
        assert_eq!(&section[..12], b"initial_data");
        // remaining bytes are null padding

        write_section(&mut elf, ".test", b"new_data").unwrap();
        let section = read_section(&elf, ".test").unwrap();
        assert!(section.starts_with(b"new_data\x00"));
    }

    #[test]
    fn test_write_overflow() {
        let elf = create_test_elf(b".test", b"short");
        let mut elf = elf;
        let result = write_section(&mut elf, ".test", b"this_is_way_too_long_for_the_section");
        assert!(result.is_err());
    }

    #[test]
    fn test_section_not_found() {
        let elf = create_test_elf(b".test", b"data");
        assert!(read_section(&elf, ".nonexistent").is_none());
    }

    #[test]
    fn test_patch_section_string() {
        let mut elf = create_test_elf(b".cfg", b"MOUNT=3AAAAAAAAA");
        patch_section_string(&mut elf, ".cfg", "MOUNT=3", "MOUNT=0").unwrap();
        let section = read_section(&elf, ".cfg").unwrap();
        let s = String::from_utf8_lossy(section);
        assert!(s.starts_with("MOUNT=0"));
    }

    /// Create a minimal ELF64 LE binary with one custom section.
    fn create_test_elf(section_name: &[u8], initial_data: &[u8]) -> Vec<u8> {
        let section_data_size = 16usize;
        let mut initial = vec![0u8; section_data_size];
        initial[..initial_data.len()].copy_from_slice(initial_data);

        // Build strtab content: "\0.shstrtab\0<section_name>\0"
        let mut strtab = Vec::new();
        strtab.push(0); // null entry
        strtab.extend_from_slice(b".shstrtab");
        strtab.push(0);
        let section_name_offset = strtab.len() as u32;
        strtab.extend_from_slice(section_name);
        strtab.push(0);

        // Layout:
        //   [0, 64):    ELF header
        //   [64, ..):   strtab data
        //   [.., ..):   section data (16 bytes)
        //   [.., end):  section headers (3 * 64 bytes)
        let ehdr_size = 64u64;
        let strtab_offset = ehdr_size;
        let section_data_offset = strtab_offset + strtab.len() as u64;
        let shdr_offset = section_data_offset + section_data_size as u64;
        let shentsize: u64 = 64;
        let shnum: u16 = 3;
        let shstrndx: u16 = 1;

        let total = shdr_offset as usize + shentsize as usize * shnum as usize;
        let mut elf = vec![0u8; total];

        // -- ELF header --
        elf[0..4].copy_from_slice(b"\x7fELF");
        elf[4] = 2; // ELFCLASS64
        elf[5] = 1; // ELFDATA2LSB
        elf[6] = 1; // EV_CURRENT
        // bytes 7-15: padding (zero)
        elf[16..18].copy_from_slice(&2u16.to_le_bytes()); // ET_EXEC
        elf[18..20].copy_from_slice(&62u16.to_le_bytes()); // EM_X86_64
        elf[20..24].copy_from_slice(&1u32.to_le_bytes()); // EV_CURRENT
        // 24-31: e_entry = 0
        // 32-39: e_phoff = 0
        elf[40..48].copy_from_slice(&shdr_offset.to_le_bytes());
        elf[48..52].copy_from_slice(&0u32.to_le_bytes()); // e_flags
        elf[52..54].copy_from_slice(&(ehdr_size as u16).to_le_bytes());
        elf[54..56].copy_from_slice(&0u16.to_le_bytes()); // e_phentsize
        elf[56..58].copy_from_slice(&0u16.to_le_bytes()); // e_phnum
        elf[58..60].copy_from_slice(&(shentsize as u16).to_le_bytes());
        elf[60..62].copy_from_slice(&shnum.to_le_bytes());
        elf[62..64].copy_from_slice(&shstrndx.to_le_bytes());

        // -- strtab data --
        let so = strtab_offset as usize;
        elf[so..so + strtab.len()].copy_from_slice(&strtab);

        // -- section data --
        let sdo = section_data_offset as usize;
        elf[sdo..sdo + section_data_size].copy_from_slice(&initial);

        // -- Section header 0: null (already zeroed) --

        // -- Section header 1: .shstrtab --
        let off = shdr_offset as usize + shentsize as usize;
        elf[off..off + 4].copy_from_slice(&1u32.to_le_bytes()); // sh_name (index 1 in strtab)
        elf[off + 4..off + 8].copy_from_slice(&3u32.to_le_bytes()); // SHT_STRTAB
        elf[off + 24..off + 32].copy_from_slice(&strtab_offset.to_le_bytes());
        elf[off + 32..off + 40].copy_from_slice(&(strtab.len() as u64).to_le_bytes());

        // -- Section header 2: custom section --
        let off = shdr_offset as usize + shentsize as usize * 2;
        elf[off..off + 4].copy_from_slice(&section_name_offset.to_le_bytes());
        elf[off + 4..off + 8].copy_from_slice(&1u32.to_le_bytes()); // SHT_PROGBITS
        elf[off + 24..off + 32].copy_from_slice(&section_data_offset.to_le_bytes());
        elf[off + 32..off + 40].copy_from_slice(&(section_data_size as u64).to_le_bytes());

        elf
    }
}
