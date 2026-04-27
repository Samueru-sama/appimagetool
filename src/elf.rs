//! Minimal, panic-resistant ELF section reader/writer used to inspect and
//! patch the embedded uruntime. Only handles ET_EXEC/ET_DYN section headers
//! — enough to find named sections and rewrite their contents in place.

use std::path::Path;

use crate::error::{Error, Result};

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
        if data.get(0..4)? != b"\x7fELF" {
            return None;
        }
        let is_64bit = *data.get(4)? == 2;
        let is_le = *data.get(5)? == 1;

        let (sh_off, sh_entsize, sh_num, sh_strndx) = if is_64bit {
            let sh_off = r64(data, 40, is_le)?;
            let sh_entsize = r16(data, 58, is_le)? as u64;
            let sh_num = r16(data, 60, is_le)? as u64;
            let sh_strndx = r16(data, 62, is_le)? as u64;
            (sh_off, sh_entsize, sh_num, sh_strndx)
        } else {
            let sh_off = r32(data, 32, is_le)? as u64;
            let sh_entsize = r16(data, 46, is_le)? as u64;
            let sh_num = r16(data, 48, is_le)? as u64;
            let sh_strndx = r16(data, 50, is_le)? as u64;
            (sh_off, sh_entsize, sh_num, sh_strndx)
        };

        if sh_off == 0 || sh_entsize == 0 || sh_num == 0 || sh_strndx >= sh_num {
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

    /// Compute the byte offset of section header `i`, returning None on overflow.
    fn shdr_offset(&self, i: u64) -> Option<usize> {
        let off = self.sh_off.checked_add(i.checked_mul(self.sh_entsize)?)?;
        usize::try_from(off).ok()
    }

    /// Get the strtab (section name string table) bytes.
    fn strtab<'a>(&self, data: &'a [u8]) -> Option<&'a [u8]> {
        let hdr_off = self.shdr_offset(self.sh_strndx)?;
        let (off, size) = self.read_offset_size(data, hdr_off)?;
        slice_range(data, off, size)
    }

    /// Find section header index whose name matches.
    fn find_section_idx(&self, data: &[u8], name: &[u8]) -> Option<u64> {
        let strtab = self.strtab(data)?;
        for i in 0..self.sh_num {
            let hdr_off = self.shdr_offset(i)?;
            let name_idx = r32(data, hdr_off, self.is_le)? as usize;
            let tail = strtab.get(name_idx..)?;
            let end = tail.iter().position(|&b| b == 0).unwrap_or(tail.len());
            if &tail[..end] == name {
                return Some(i);
            }
        }
        None
    }

    /// Get (offset, size) of a section by index.
    fn section_offset_size(&self, data: &[u8], idx: u64) -> Option<(usize, usize)> {
        let hdr_off = self.shdr_offset(idx)?;
        self.read_offset_size(data, hdr_off)
    }

    /// Read sh_offset and sh_size from a section header at `hdr_off`.
    fn read_offset_size(&self, data: &[u8], hdr_off: usize) -> Option<(usize, usize)> {
        if self.is_64bit {
            let off = usize::try_from(r64(data, hdr_off.checked_add(24)?, self.is_le)?).ok()?;
            let size = usize::try_from(r64(data, hdr_off.checked_add(32)?, self.is_le)?).ok()?;
            Some((off, size))
        } else {
            let off = r32(data, hdr_off.checked_add(16)?, self.is_le)? as usize;
            let size = r32(data, hdr_off.checked_add(20)?, self.is_le)? as usize;
            Some((off, size))
        }
    }
}

/// Read the value of an ELF section by name from a binary.
pub fn read_section<'a>(data: &'a [u8], name: &str) -> Option<&'a [u8]> {
    let info = ElfInfo::parse(data)?;
    let idx = info.find_section_idx(data, name.as_bytes())?;
    let (off, size) = info.section_offset_size(data, idx)?;
    slice_range(data, off, size)
}

/// Find the offset and size of an ELF section by name.
pub fn find_section(data: &[u8], name: &str) -> Option<(usize, usize)> {
    let info = ElfInfo::parse(data)?;
    let idx = info.find_section_idx(data, name.as_bytes())?;
    info.section_offset_size(data, idx)
}

/// Write data into an ELF section by name. The data is null-padded to fill the section.
pub fn write_section(data: &mut [u8], name: &str, value: &[u8]) -> Result<()> {
    let info = ElfInfo::parse(data).ok_or(Error::MalformedElf)?;

    let idx = info
        .find_section_idx(data, name.as_bytes())
        .ok_or_else(|| Error::SectionNotFound(name.to_string()))?;

    let (sec_offset, sec_size) = info
        .section_offset_size(data, idx)
        .ok_or(Error::MalformedElf)?;

    let end = sec_offset
        .checked_add(sec_size)
        .ok_or(Error::MalformedElf)?;
    let region = data.get_mut(sec_offset..end).ok_or(Error::MalformedElf)?;

    if value.len() > region.len() {
        return Err(Error::SectionOverflow {
            name: name.to_string(),
            size: value.len(),
            capacity: region.len(),
        });
    }

    region.fill(0);
    region[..value.len()].copy_from_slice(value);
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
/// Returns `Ok(true)` if the pattern was found and replaced, `Ok(false)` otherwise.
pub fn patch_section_string(
    data: &mut [u8],
    section_name: &str,
    pattern: &str,
    replacement: &str,
) -> Result<bool> {
    let (offset, size) = find_section(data, section_name)
        .ok_or_else(|| Error::SectionNotFound(section_name.to_string()))?;

    let end = offset.checked_add(size).ok_or(Error::MalformedElf)?;
    let section = data.get(offset..end).ok_or(Error::MalformedElf)?;
    let section_str = String::from_utf8_lossy(section);

    let Some(pos) = section_str.find(pattern) else {
        return Ok(false);
    };

    let byte_pos = offset.checked_add(pos).ok_or(Error::MalformedElf)?;
    let pat_len = pattern.len();
    let rep_len = replacement.len().min(pat_len);
    let pat_end = byte_pos.checked_add(pat_len).ok_or(Error::MalformedElf)?;
    let region = data.get_mut(byte_pos..pat_end).ok_or(Error::MalformedElf)?;
    region[..rep_len].copy_from_slice(&replacement.as_bytes()[..rep_len]);
    if rep_len < pat_len {
        region[rep_len..].fill(0);
    }
    Ok(true)
}

/// Helper: bounds-checked subslice with explicit length.
fn slice_range(data: &[u8], off: usize, size: usize) -> Option<&[u8]> {
    let end = off.checked_add(size)?;
    data.get(off..end)
}

// Helpers for reading multi-byte values at a given offset.
fn r16(data: &[u8], off: usize, le: bool) -> Option<u16> {
    let bytes: [u8; 2] = data.get(off..off.checked_add(2)?)?.try_into().ok()?;
    Some(if le {
        u16::from_le_bytes(bytes)
    } else {
        u16::from_be_bytes(bytes)
    })
}

fn r32(data: &[u8], off: usize, le: bool) -> Option<u32> {
    let bytes: [u8; 4] = data.get(off..off.checked_add(4)?)?.try_into().ok()?;
    Some(if le {
        u32::from_le_bytes(bytes)
    } else {
        u32::from_be_bytes(bytes)
    })
}

fn r64(data: &[u8], off: usize, le: bool) -> Option<u64> {
    let bytes: [u8; 8] = data.get(off..off.checked_add(8)?)?.try_into().ok()?;
    Some(if le {
        u64::from_le_bytes(bytes)
    } else {
        u64::from_be_bytes(bytes)
    })
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
        let patched = patch_section_string(&mut elf, ".cfg", "MOUNT=3", "MOUNT=0").unwrap();
        assert!(patched, "patch should report success when pattern matches");
        let section = read_section(&elf, ".cfg").unwrap();
        let s = String::from_utf8_lossy(section);
        assert!(s.starts_with("MOUNT=0"));
    }

    #[test]
    fn test_patch_section_string_no_match() {
        let mut elf = create_test_elf(b".cfg", b"MOUNT=3AAAAAAAAA");
        let patched = patch_section_string(&mut elf, ".cfg", "ABSENT=9", "ABSENT=0").unwrap();
        assert!(!patched, "patch should report no-op when pattern is absent");
    }

    #[test]
    fn test_parse_rejects_short_input() {
        // Truncated header — must not panic, must reject.
        assert!(read_section(b"", ".test").is_none());
        assert!(read_section(b"\x7fELF", ".test").is_none());
        assert!(read_section(&[0u8; 32], ".test").is_none());
    }

    #[test]
    fn test_parse_rejects_garbled_section_offsets() {
        // A header with sh_off pointing past EOF must not panic.
        let mut elf = create_test_elf(b".test", b"data");
        // Overwrite e_shoff with a huge value.
        elf[40..48].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(read_section(&elf, ".test").is_none());
        assert!(write_section(&mut elf.clone(), ".test", b"x").is_err());
    }

    #[test]
    fn test_parse_rejects_oversized_strndx() {
        let mut elf = create_test_elf(b".test", b"data");
        // sh_strndx >= sh_num must be rejected.
        elf[62..64].copy_from_slice(&999u16.to_le_bytes());
        assert!(read_section(&elf, ".test").is_none());
    }

    #[test]
    fn test_random_garbage_does_not_panic() {
        // A bytestream that starts with the ELF magic but is otherwise junk.
        let mut elf = vec![0u8; 256];
        elf[0..4].copy_from_slice(b"\x7fELF");
        elf[4] = 2;
        elf[5] = 1;
        for byte in elf.iter_mut().skip(6) {
            *byte = 0xff;
        }
        // Must not panic; either parses to None or returns None on lookup.
        let _ = read_section(&elf, ".test");
        let _ = find_section(&elf, ".test");
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
