//! Embeds the Windows executable icon (`RT_GROUP_ICON` / `RT_ICON`) and PE version metadata
//! (`RT_VERSION`) directly as a standard COFF `.rsrc` object without requiring `rc.exe` or
//! `windres` on PATH. No-op on non-Windows targets.
use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn main() {
    println!("cargo:rerun-if-changed=assets/brand/covalos-icon.ico");
    println!("cargo:rerun-if-changed=Cargo.toml");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let ico_path = manifest_dir.join("assets/brand/covalos-icon.ico");
    let ico_bytes = fs::read(&ico_path).expect("read assets/brand/covalos-icon.ico");
    let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.4.0".into());

    let obj_bytes = build_rsrc_coff(&ico_bytes, &version);
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let obj_path = out_dir.join("covalos_rsrc.o");
    write_if_changed(&obj_path, &obj_bytes);
    println!("cargo:rustc-link-arg={}", obj_path.display());
}

fn write_if_changed(path: &Path, bytes: &[u8]) {
    if fs::read(path).ok().as_deref() != Some(bytes) {
        fs::write(path, bytes).expect("write Windows resource COFF object");
    }
}

struct IcoFrame<'a> {
    width: u8,
    height: u8,
    colors: u8,
    reserved: u8,
    planes: u16,
    bit_count: u16,
    data: &'a [u8],
}

fn parse_ico(bytes: &[u8]) -> Vec<IcoFrame<'_>> {
    assert!(bytes.len() >= 6, "ICO header truncated");
    let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
    assert!(bytes.len() >= 6 + count * 16, "ICO directory truncated");
    let mut frames = Vec::with_capacity(count);
    for i in 0..count {
        let off = 6 + i * 16;
        let size = u32::from_le_bytes(bytes[off + 8..off + 12].try_into().unwrap()) as usize;
        let data_off = u32::from_le_bytes(bytes[off + 12..off + 16].try_into().unwrap()) as usize;
        frames.push(IcoFrame {
            width: bytes[off],
            height: bytes[off + 1],
            colors: bytes[off + 2],
            reserved: bytes[off + 3],
            planes: u16::from_le_bytes([bytes[off + 4], bytes[off + 5]]),
            bit_count: u16::from_le_bytes([bytes[off + 6], bytes[off + 7]]),
            data: &bytes[data_off..data_off + size],
        });
    }
    frames
}

fn build_group_icon(frames: &[IcoFrame<'_>]) -> Vec<u8> {
    let mut out = Vec::with_capacity(6 + frames.len() * 14);
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // type = icon
    out.extend_from_slice(&(frames.len() as u16).to_le_bytes());
    for (idx, f) in frames.iter().enumerate() {
        out.push(f.width);
        out.push(f.height);
        out.push(f.colors);
        out.push(f.reserved);
        out.extend_from_slice(&f.planes.to_le_bytes());
        out.extend_from_slice(&f.bit_count.to_le_bytes());
        out.extend_from_slice(&(f.data.len() as u32).to_le_bytes());
        out.extend_from_slice(&((idx + 1) as u16).to_le_bytes());
    }
    out
}

fn push_utf16z(out: &mut Vec<u8>, s: &str) {
    for u in s.encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out.extend_from_slice(&0u16.to_le_bytes());
}

fn align4(out: &mut Vec<u8>) {
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

fn ver_block(
    key: &str,
    value_type: u16,
    value_len_units: u16,
    value: &[u8],
    children: &[Vec<u8>],
) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes()); // placeholder wLength
    out.extend_from_slice(&value_len_units.to_le_bytes());
    out.extend_from_slice(&value_type.to_le_bytes());
    push_utf16z(&mut out, key);
    align4(&mut out);
    out.extend_from_slice(value);
    for child in children {
        align4(&mut out);
        out.extend_from_slice(child);
    }
    let len = out.len() as u16;
    out[0..2].copy_from_slice(&len.to_le_bytes());
    out
}

fn ver_string(key: &str, val: &str) -> Vec<u8> {
    let mut v = Vec::new();
    push_utf16z(&mut v, val);
    let chars = (v.len() / 2) as u16;
    ver_block(key, 1, chars, &v, &[])
}

fn build_version_info(version: &str) -> Vec<u8> {
    let nums: Vec<u16> = version
        .split('.')
        .map(|p| p.parse::<u16>().unwrap_or(0))
        .collect();
    let major = nums.first().copied().unwrap_or(0);
    let minor = nums.get(1).copied().unwrap_or(4);
    let patch = nums.get(2).copied().unwrap_or(0);
    let ver_str = format!("{major}.{minor}.{patch}.0");

    let mut fixed = Vec::with_capacity(52);
    for word in [
        0xFEEF_04BDu32,                          // dwSignature
        0x0001_0000,                             // dwStrucVersion
        ((major as u32) << 16) | (minor as u32), // dwFileVersionMS
        (patch as u32) << 16,                    // dwFileVersionLS
        ((major as u32) << 16) | (minor as u32), // dwProductVersionMS
        (patch as u32) << 16,                    // dwProductVersionLS
        0x0000_003F,                             // dwFileFlagsMask
        0x0000_0000,                             // dwFileFlags
        0x0004_0004,                             // VOS_NT_WINDOWS32
        0x0000_0001,                             // VFT_APP
        0x0000_0000,                             // dwFileSubtype
        0x0000_0000,                             // dwFileDateMS
        0x0000_0000,                             // dwFileDateLS
    ] {
        fixed.extend_from_slice(&word.to_le_bytes());
    }

    let strings = vec![
        ver_string("FileDescription", "COVALOS IDE"),
        ver_string("FileVersion", &ver_str),
        ver_string("InternalName", "covalos"),
        ver_string("OriginalFilename", "covalos.exe"),
        ver_string("ProductName", "COVALOS"),
        ver_string("ProductVersion", &ver_str),
    ];
    let string_table = ver_block("040904B0", 1, 0, &[], &strings);
    let string_file_info = ver_block("StringFileInfo", 1, 0, &[], &[string_table]);

    let translation = [0x09u8, 0x04, 0xB0, 0x04]; // en-US (0x0409) + Unicode (0x04B0)
    let var_entry = ver_block("Translation", 0, 4, &translation, &[]);
    let var_file_info = ver_block("VarFileInfo", 1, 0, &[], &[var_entry]);

    ver_block(
        "VS_VERSION_INFO",
        0,
        fixed.len() as u16,
        &fixed,
        &[string_file_info, var_file_info],
    )
}

/// Resource leaf: (type_id, name_id, lang_id, payload)
struct Leaf {
    type_id: u32,
    name_id: u32,
    lang_id: u32,
    payload: Vec<u8>,
}

fn dir_header(id_entries: u16) -> [u8; 16] {
    let mut h = [0u8; 16];
    h[14..16].copy_from_slice(&id_entries.to_le_bytes());
    h
}

/// Build an x86_64 Windows COFF object containing a single `.rsrc` section with
/// `IMAGE_REL_AMD64_ADDR32NB` relocations for every resource data entry.
fn build_rsrc_coff(ico_bytes: &[u8], version: &str) -> Vec<u8> {
    let frames = parse_ico(ico_bytes);
    let group = build_group_icon(&frames);
    let ver = build_version_info(version);

    // Group leaves by type_id in ascending order (required by PE resource spec):
    // 3 = RT_ICON, 14 = RT_GROUP_ICON, 16 = RT_VERSION.
    let mut leaves = Vec::new();
    for (idx, f) in frames.iter().enumerate() {
        leaves.push(Leaf {
            type_id: 3,
            name_id: (idx + 1) as u32,
            lang_id: 0x0409,
            payload: f.data.to_vec(),
        });
    }
    leaves.push(Leaf {
        type_id: 14,
        name_id: 1,
        lang_id: 0x0409,
        payload: group,
    });
    leaves.push(Leaf {
        type_id: 16,
        name_id: 1,
        lang_id: 0x0409,
        payload: ver,
    });

    // Collect distinct sorted type IDs.
    let mut types: Vec<u32> = leaves.iter().map(|l| l.type_id).collect();
    types.sort_unstable();
    types.dedup();

    // Pre-calculate directory layout inside `.rsrc`:
    // [root dir] -> [type dirs] -> [name dirs] -> [data entries] -> [payload blobs]
    let root_size = 16 + types.len() * 8;
    let mut type_dir_offsets = Vec::with_capacity(types.len());
    let mut cursor = root_size;
    for &t in &types {
        type_dir_offsets.push(cursor);
        let count = leaves.iter().filter(|l| l.type_id == t).count();
        cursor += 16 + count * 8;
    }
    let mut name_dir_offsets = Vec::with_capacity(leaves.len());
    for _ in &leaves {
        name_dir_offsets.push(cursor);
        cursor += 16 + 8; // 1 language entry per leaf
    }
    let data_entries_offset = cursor;
    cursor += leaves.len() * 16;
    while !cursor.is_multiple_of(4) {
        cursor += 1;
    }
    let mut payload_offsets = Vec::with_capacity(leaves.len());
    for leaf in &leaves {
        payload_offsets.push(cursor);
        cursor += leaf.payload.len();
        while !cursor.is_multiple_of(4) {
            cursor += 1;
        }
    }

    let mut rsrc = Vec::with_capacity(cursor);
    // Root directory
    rsrc.extend_from_slice(&dir_header(types.len() as u16));
    for (idx, &t) in types.iter().enumerate() {
        rsrc.extend_from_slice(&t.to_le_bytes());
        rsrc.extend_from_slice(&(0x8000_0000u32 | type_dir_offsets[idx] as u32).to_le_bytes());
    }
    // Type directories
    for &t in &types {
        let indices: Vec<usize> = leaves
            .iter()
            .enumerate()
            .filter(|(_, l)| l.type_id == t)
            .map(|(i, _)| i)
            .collect();
        rsrc.extend_from_slice(&dir_header(indices.len() as u16));
        for i in indices {
            rsrc.extend_from_slice(&leaves[i].name_id.to_le_bytes());
            rsrc.extend_from_slice(&(0x8000_0000u32 | name_dir_offsets[i] as u32).to_le_bytes());
        }
    }
    // Name directories (each points to one Data Entry)
    for (i, leaf) in leaves.iter().enumerate() {
        rsrc.extend_from_slice(&dir_header(1));
        rsrc.extend_from_slice(&leaf.lang_id.to_le_bytes());
        let entry_off = (data_entries_offset + i * 16) as u32;
        rsrc.extend_from_slice(&entry_off.to_le_bytes());
    }
    // Data entries (record relocation offset for each RVA field)
    let mut relocs = Vec::with_capacity(leaves.len());
    for (i, leaf) in leaves.iter().enumerate() {
        let reloc_va = rsrc.len() as u32;
        relocs.push(reloc_va);
        rsrc.extend_from_slice(&(payload_offsets[i] as u32).to_le_bytes()); // offset within .rsrc
        rsrc.extend_from_slice(&(leaf.payload.len() as u32).to_le_bytes());
        rsrc.extend_from_slice(&0u32.to_le_bytes()); // CodePage
        rsrc.extend_from_slice(&0u32.to_le_bytes()); // Reserved
    }
    align4(&mut rsrc);
    for leaf in &leaves {
        rsrc.extend_from_slice(&leaf.payload);
        align4(&mut rsrc);
    }

    // Wrap `.rsrc` into a minimal x86_64 COFF object file.
    // Layout: [20-byte COFF header][40-byte section header][rsrc bytes][relocs][1 symbol][4-byte strtab]
    let raw_data_ptr = 60u32;
    let reloc_ptr = raw_data_ptr + rsrc.len() as u32;
    let symtab_ptr = reloc_ptr + (relocs.len() * 10) as u32;

    let mut obj = Vec::with_capacity(symtab_ptr as usize + 22);
    // COFF File Header (20 bytes)
    obj.extend_from_slice(&0x8664u16.to_le_bytes()); // IMAGE_FILE_MACHINE_AMD64
    obj.extend_from_slice(&1u16.to_le_bytes()); // NumberOfSections
    obj.extend_from_slice(&0u32.to_le_bytes()); // TimeDateStamp
    obj.extend_from_slice(&symtab_ptr.to_le_bytes()); // PointerToSymbolTable
    obj.extend_from_slice(&1u32.to_le_bytes()); // NumberOfSymbols
    obj.extend_from_slice(&0u16.to_le_bytes()); // SizeOfOptionalHeader
    obj.extend_from_slice(&0x0004u16.to_le_bytes()); // IMAGE_FILE_LINE_NUMS_STRIPPED

    // Section Header `.rsrc` (40 bytes)
    obj.extend_from_slice(b".rsrc\0\0\0");
    obj.extend_from_slice(&0u32.to_le_bytes()); // VirtualSize
    obj.extend_from_slice(&0u32.to_le_bytes()); // VirtualAddress
    obj.extend_from_slice(&(rsrc.len() as u32).to_le_bytes()); // SizeOfRawData
    obj.extend_from_slice(&raw_data_ptr.to_le_bytes()); // PointerToRawData
    obj.extend_from_slice(&reloc_ptr.to_le_bytes()); // PointerToRelocations
    obj.extend_from_slice(&0u32.to_le_bytes()); // PointerToLinenumbers
    obj.extend_from_slice(&(relocs.len() as u16).to_le_bytes()); // NumberOfRelocations
    obj.extend_from_slice(&0u16.to_le_bytes()); // NumberOfLinenumbers
    // IMAGE_SCN_CNT_INITIALIZED_DATA | IMAGE_SCN_MEM_READ
    obj.extend_from_slice(&0x4000_0040u32.to_le_bytes());

    // Raw `.rsrc` data
    obj.extend_from_slice(&rsrc);

    // Relocations: IMAGE_REL_AMD64_ADDR32NB (3) against symbol 0 (`.rsrc`)
    for va in relocs {
        obj.extend_from_slice(&va.to_le_bytes());
        obj.extend_from_slice(&0u32.to_le_bytes()); // SymbolTableIndex = 0
        obj.extend_from_slice(&3u16.to_le_bytes()); // IMAGE_REL_AMD64_ADDR32NB
    }

    // Symbol table: 1 section symbol for `.rsrc` (18 bytes)
    obj.extend_from_slice(b".rsrc\0\0\0");
    obj.extend_from_slice(&0u32.to_le_bytes()); // Value
    obj.extend_from_slice(&1i16.to_le_bytes()); // SectionNumber = 1
    obj.extend_from_slice(&0u16.to_le_bytes()); // Type
    obj.push(3); // IMAGE_SYM_CLASS_STATIC
    obj.push(0); // NumberOfAuxSymbols

    // Empty string table (4-byte size = 4)
    obj.extend_from_slice(&4u32.to_le_bytes());
    obj
}
