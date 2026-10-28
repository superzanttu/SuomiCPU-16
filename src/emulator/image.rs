use crate::CpuError;
use std::fs;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

const MAGIC: &[u8; 8] = b"SC16IMG2";
const PREVIOUS_MAGIC: &[u8; 8] = b"SC16IMG1";
const FORMAT_VERSION: u16 = 2;
const HEADER_SIZE: u64 = 20;
const SEGMENT_HEADER_SIZE: u64 = 20;
const MAX_SEGMENTS: usize = 65_536;
const MEMORY_SIZE: u64 = 1 << 30;
const PERMISSION_READ: u8 = 1;
const PERMISSION_WRITE: u8 = 2;
const PERMISSION_EXECUTE: u8 = 4;
const PERMISSION_MASK: u8 = PERMISSION_READ | PERMISSION_WRITE | PERMISSION_EXECUTE;

#[derive(Debug)]
pub enum ImageError {
    Io(io::Error),
    UnsupportedC(String),
    CCompiler(String),
    Assembly(String),
    InvalidFormat(&'static str),
    Cpu(CpuError),
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::UnsupportedC(source) => write!(
                formatter,
                "the Rust C compiler does not yet support this C program: {source}"
            ),
            Self::CCompiler(error) => write!(formatter, "Rust C compiler error: {error}"),
            Self::Assembly(error) => write!(formatter, "SC-16 assembly failed: {error}"),
            Self::InvalidFormat(reason) => write!(formatter, "invalid SC-16 image: {reason}"),
            Self::Cpu(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for ImageError {}

impl From<io::Error> for ImageError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<CpuError> for ImageError {
    fn from(error: CpuError) -> Self {
        Self::Cpu(error)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ProgramImage {
    pub entry_point: u32,
    pub segments: Vec<ImageSegment>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegmentPermissions {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

impl SegmentPermissions {
    pub const READ_WRITE_EXECUTE: Self = Self {
        read: true,
        write: true,
        execute: true,
    };

    fn bits(self) -> u8 {
        u8::from(self.read) * PERMISSION_READ
            | u8::from(self.write) * PERMISSION_WRITE
            | u8::from(self.execute) * PERMISSION_EXECUTE
    }

    fn from_bits(bits: u8) -> Result<Self, ImageError> {
        if bits & !PERMISSION_MASK != 0 {
            return Err(ImageError::InvalidFormat(
                "segment contains unknown permission flags",
            ));
        }
        Ok(Self {
            read: bits & PERMISSION_READ != 0,
            write: bits & PERMISSION_WRITE != 0,
            execute: bits & PERMISSION_EXECUTE != 0,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageSegment {
    pub address: u32,
    pub data: Vec<u8>,
    pub alignment: u32,
    pub permissions: SegmentPermissions,
}

impl ImageSegment {
    pub fn new(address: u32, data: Vec<u8>) -> Self {
        Self {
            address,
            data,
            alignment: 1,
            permissions: SegmentPermissions::READ_WRITE_EXECUTE,
        }
    }
}

impl crate::cpu::LoadableSegment for ImageSegment {
    fn address(&self) -> u32 {
        self.address
    }

    fn data(&self) -> &[u8] {
        &self.data
    }

    fn image_metadata(&self) -> Option<(u32, SegmentPermissions)> {
        Some((self.alignment, self.permissions))
    }
}

fn validate_image(image: &ProgramImage) -> Result<(), ImageError> {
    if image.segments.is_empty() || image.segments.len() > MAX_SEGMENTS {
        return Err(ImageError::InvalidFormat("invalid segment count"));
    }
    if u64::from(image.entry_point) >= MEMORY_SIZE {
        return Err(ImageError::InvalidFormat(
            "entry point is outside SC-16 memory",
        ));
    }
    let mut total = 0u64;
    let mut entry_is_executable = false;
    let mut ranges = Vec::with_capacity(image.segments.len());
    for segment in &image.segments {
        if segment.data.is_empty() {
            return Err(ImageError::InvalidFormat("empty load segment"));
        }
        if segment.alignment == 0
            || !segment.alignment.is_power_of_two()
            || u64::from(segment.alignment) > MEMORY_SIZE
            || segment.address % segment.alignment != 0
        {
            return Err(ImageError::InvalidFormat("invalid segment alignment"));
        }
        if segment.permissions.bits() == 0 {
            return Err(ImageError::InvalidFormat("load segment has no permissions"));
        }
        let length = segment.data.len() as u64;
        let start = u64::from(segment.address);
        let end = start
            .checked_add(length)
            .ok_or(ImageError::InvalidFormat("segment range overflow"))?;
        if end > MEMORY_SIZE {
            return Err(ImageError::InvalidFormat("segment is outside SC-16 memory"));
        }
        if u64::from(segment.address) <= u64::from(image.entry_point)
            && u64::from(image.entry_point) < end
            && segment.permissions.execute
        {
            entry_is_executable = true;
        }
        ranges.push((start, end));
        total = total
            .checked_add(length)
            .ok_or(ImageError::InvalidFormat("total image size overflow"))?;
        if total > MEMORY_SIZE {
            return Err(ImageError::InvalidFormat("image exceeds memory"));
        }
    }
    ranges.sort_unstable_by_key(|(start, _)| *start);
    if ranges.windows(2).any(|pair| pair[1].0 < pair[0].1) {
        return Err(ImageError::InvalidFormat("load segments overlap"));
    }
    if !entry_is_executable {
        return Err(ImageError::InvalidFormat(
            "entry point is not inside an executable load segment",
        ));
    }
    Ok(())
}

pub fn load_binary(path: &Path) -> Result<ProgramImage, ImageError> {
    let mut file = fs::File::open(path)?;
    let file_length = file.metadata()?.len();
    let mut magic = [0; MAGIC.len()];
    if file_length >= magic.len() as u64 {
        file.read_exact(&mut magic)?;
        if &magic == MAGIC {
            return read_image(path);
        }
        if &magic == PREVIOUS_MAGIC {
            return Err(ImageError::InvalidFormat("unsupported image version"));
        }
    }
    if file_length > MEMORY_SIZE {
        return Err(ImageError::InvalidFormat("binary exceeds SC-16 memory"));
    }
    let capacity = usize::try_from(file_length)
        .map_err(|_| ImageError::InvalidFormat("binary exceeds SC-16 memory"))?;
    file.seek(SeekFrom::Start(0))?;
    let mut data = Vec::new();
    data.try_reserve_exact(capacity)
        .map_err(|_| ImageError::InvalidFormat("binary allocation failed"))?;
    file.take(MEMORY_SIZE + 1).read_to_end(&mut data)?;
    if data.len() as u64 > MEMORY_SIZE {
        return Err(ImageError::InvalidFormat("binary exceeds SC-16 memory"));
    }
    let image = ProgramImage {
        entry_point: 0,
        segments: vec![ImageSegment::new(0, data)],
    };
    validate_image(&image)?;
    Ok(image)
}

pub fn load_program(path: &Path) -> Result<ProgramImage, ImageError> {
    let extension = path.extension().and_then(|extension| extension.to_str());
    if extension.is_some_and(|extension| extension.eq_ignore_ascii_case("bin")) {
        return load_binary(path);
    }
    if extension.is_some_and(|extension| extension.eq_ignore_ascii_case("json")) {
        return Err(ImageError::InvalidFormat(
            "JSON files are data; load them through the disk API",
        ));
    }
    if extension.is_some_and(|extension| {
        extension.eq_ignore_ascii_case("img") || extension.eq_ignore_ascii_case("sc16img")
    }) {
        return read_image(path);
    }
    if extension.is_some_and(|extension| extension.eq_ignore_ascii_case("c")) {
        let tank16_main = crate::tank16_map::default_game_dir().join("main.c");
        if path.canonicalize().ok() == tank16_main.canonicalize().ok() {
            return crate::tank16_map::compile_runtime_tank16(
                &crate::tank16_map::default_game_dir(),
            )
            .map_err(ImageError::CCompiler);
        }
    }
    build_source(path)
}

pub fn load_prebuilt_program(path: &Path) -> Result<ProgramImage, ImageError> {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some(extension) if extension.eq_ignore_ascii_case("bin") => load_binary(path),
        Some(extension)
            if extension.eq_ignore_ascii_case("img")
                || extension.eq_ignore_ascii_case("sc16img") =>
        {
            read_image(path)
        }
        _ => Err(ImageError::InvalidFormat(
            "runtime requires a prebuilt .bin or .sc16img image; compile source with sc16build",
        )),
    }
}

pub fn write_image(path: &Path, image: &ProgramImage) -> Result<(), ImageError> {
    fs::write(path, encode_image(image)?)?;
    Ok(())
}

pub fn encode_image(image: &ProgramImage) -> Result<Vec<u8>, ImageError> {
    validate_image(image)?;
    let segment_count = u32::try_from(image.segments.len())
        .map_err(|_| ImageError::InvalidFormat("segment count is excessive"))?;
    let mut file = Vec::new();
    file.write_all(MAGIC)?;
    file.write_all(&FORMAT_VERSION.to_be_bytes())?;
    file.write_all(&0u16.to_be_bytes())?;
    file.write_all(&image.entry_point.to_be_bytes())?;
    file.write_all(&segment_count.to_be_bytes())?;
    for segment in &image.segments {
        file.write_all(&segment.address.to_be_bytes())?;
        file.write_all(&(segment.data.len() as u64).to_be_bytes())?;
        file.write_all(&segment.alignment.to_be_bytes())?;
        file.write_all(&[segment.permissions.bits(), 0])?;
        file.write_all(&0u16.to_be_bytes())?;
        file.write_all(&segment.data)?;
    }
    Ok(file)
}

pub fn build_source(source: &Path) -> Result<ProgramImage, ImageError> {
    if source
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("asm"))
    {
        return crate::assembler::assemble_file(source, 0)
            .map_err(|error| ImageError::Assembly(error.to_string()));
    }
    if source
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("c"))
    {
        let source_text = fs::read_to_string(source)?;
        let source_text = source_text.strip_prefix('\u{feff}').unwrap_or(&source_text);
        if let Some(image) =
            crate::c_compiler::compile_minimal_program(source_text, &source.to_string_lossy())
                .map_err(|error| ImageError::CCompiler(error.to_string()))?
        {
            return Ok(image);
        }
        return Err(ImageError::UnsupportedC(source.display().to_string()));
    }
    Err(ImageError::InvalidFormat(
        "source must be assembly or Rust-supported C",
    ))
}

pub fn build_kernel_with_embedded_c_source(
    kernel_source: &Path,
    user_source: &Path,
) -> Result<ProgramImage, ImageError> {
    let kernel = fs::read_to_string(kernel_source)?;
    let user_assembly = compile_c_assembly_at(user_source, 0x10000)?;
    crate::assembler::assemble(&format!("{kernel}\n{user_assembly}"), 0)
        .map_err(|error| ImageError::Assembly(error.to_string()))
}

pub fn build_c_program_at(source: &Path, base: u32) -> Result<ProgramImage, ImageError> {
    let user_assembly = compile_c_assembly_at(source, base)?;
    crate::assembler::assemble(&user_assembly, 0)
        .map_err(|error| ImageError::Assembly(error.to_string()))
}

fn compile_c_assembly_at(source: &Path, base: u32) -> Result<String, ImageError> {
    let user = fs::read_to_string(source)?;
    let user = user.strip_prefix('\u{feff}').unwrap_or(&user);
    let Some(user_assembly) =
        crate::c_compiler::compile_minimal_program_to_assembly(user, &source.to_string_lossy())
            .map_err(|error| ImageError::CCompiler(error.to_string()))?
    else {
        return Err(ImageError::UnsupportedC(source.display().to_string()));
    };
    relocate_c_assembly(&user_assembly, base).map_err(ImageError::Assembly)
}

fn relocate_c_assembly(source: &str, base: u32) -> Result<String, String> {
    let mut relocated = String::with_capacity(source.len());
    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(address) = trimmed.strip_prefix(".address ") {
            let address = if let Some(hex) = address.strip_prefix("0x") {
                u32::from_str_radix(hex, 16)
            } else {
                address.parse()
            }
            .map_err(|_| format!("invalid compiler address directive: {trimmed}"))?;
            let address = address
                .checked_add(base)
                .ok_or_else(|| format!("embedded C address overflows: {trimmed}"))?;
            relocated.push_str(&format!(".address {address:#x}\n"));
        } else if trimmed == "JMP __c_boot" {
            relocated.push_str("    JMPX __c_boot\n");
        } else {
            relocated.push_str(line);
            relocated.push('\n');
        }
    }
    Ok(relocated)
}

pub fn read_image(path: &Path) -> Result<ProgramImage, ImageError> {
    let mut file = fs::File::open(path)?;
    let file_length = file.metadata()?.len();
    if file_length < HEADER_SIZE {
        return Err(ImageError::InvalidFormat("truncated image header"));
    }
    let mut header = [0u8; HEADER_SIZE as usize];
    file.read_exact(&mut header)?;
    if &header[..8] == PREVIOUS_MAGIC {
        return Err(ImageError::InvalidFormat("unsupported image version"));
    }
    if &header[..8] != MAGIC {
        return Err(ImageError::InvalidFormat("bad magic"));
    }
    let version = u16::from_be_bytes(header[8..10].try_into().unwrap());
    if version != FORMAT_VERSION {
        return Err(ImageError::InvalidFormat("unsupported image version"));
    }
    if u16::from_be_bytes(header[10..12].try_into().unwrap()) != 0 {
        return Err(ImageError::InvalidFormat("nonzero reserved header flags"));
    }
    let entry_point = u32::from_be_bytes(header[12..16].try_into().unwrap());
    let segment_count = u32::from_be_bytes(header[16..20].try_into().unwrap()) as usize;
    if segment_count == 0 || segment_count > MAX_SEGMENTS {
        return Err(ImageError::InvalidFormat("segment count is excessive"));
    }
    let minimum_length = HEADER_SIZE
        .checked_add(
            (segment_count as u64)
                .checked_mul(SEGMENT_HEADER_SIZE)
                .ok_or(ImageError::InvalidFormat("image size overflow"))?,
        )
        .ok_or(ImageError::InvalidFormat("image size overflow"))?;
    if file_length < minimum_length {
        return Err(ImageError::InvalidFormat("truncated image"));
    }
    let mut segments = Vec::new();
    segments
        .try_reserve_exact(segment_count)
        .map_err(|_| ImageError::InvalidFormat("image segment allocation failed"))?;
    let mut total = 0u64;
    let mut consumed = HEADER_SIZE;
    for _ in 0..segment_count {
        let mut segment_header = [0u8; SEGMENT_HEADER_SIZE as usize];
        file.read_exact(&mut segment_header)?;
        consumed = consumed
            .checked_add(SEGMENT_HEADER_SIZE)
            .ok_or(ImageError::InvalidFormat("image size overflow"))?;
        let address = u32::from_be_bytes(segment_header[..4].try_into().unwrap());
        let length = u64::from_be_bytes(segment_header[4..12].try_into().unwrap());
        let alignment = u32::from_be_bytes(segment_header[12..16].try_into().unwrap());
        let permissions = SegmentPermissions::from_bits(segment_header[16])?;
        if segment_header[17..20] != [0, 0, 0] {
            return Err(ImageError::InvalidFormat("nonzero reserved segment fields"));
        }
        let end = u64::from(address)
            .checked_add(length)
            .ok_or(ImageError::InvalidFormat("segment range overflow"))?;
        if end > MEMORY_SIZE || length > MEMORY_SIZE {
            return Err(ImageError::InvalidFormat("segment is outside SC-16 memory"));
        }
        total = total
            .checked_add(length)
            .ok_or(ImageError::InvalidFormat("total image size overflow"))?;
        if total > MEMORY_SIZE {
            return Err(ImageError::InvalidFormat("image exceeds memory"));
        }
        consumed = consumed
            .checked_add(length)
            .ok_or(ImageError::InvalidFormat("image size overflow"))?;
        if consumed > file_length {
            return Err(ImageError::InvalidFormat("truncated segment data"));
        }
        let length = usize::try_from(length)
            .map_err(|_| ImageError::InvalidFormat("segment is too large"))?;
        let mut data = Vec::new();
        data.try_reserve_exact(length)
            .map_err(|_| ImageError::InvalidFormat("segment allocation failed"))?;
        data.resize(length, 0);
        file.read_exact(&mut data)?;
        segments.push(ImageSegment {
            address,
            data,
            alignment,
            permissions,
        });
    }
    let mut trailing = [0; 1];
    if consumed != file_length || file.read(&mut trailing)? != 0 {
        return Err(ImageError::InvalidFormat("trailing bytes"));
    }
    if file.metadata()?.len() != consumed {
        return Err(ImageError::InvalidFormat("image changed while reading"));
    }
    let image = ProgramImage {
        entry_point,
        segments,
    };
    validate_image(&image)?;
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_image(entry_point: u32, segments: &[Vec<u8>]) -> Vec<u8> {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&FORMAT_VERSION.to_be_bytes());
        bytes.extend_from_slice(&0u16.to_be_bytes());
        bytes.extend_from_slice(&entry_point.to_be_bytes());
        bytes.extend_from_slice(&(segments.len() as u32).to_be_bytes());
        for segment in segments {
            bytes.extend_from_slice(segment);
        }
        bytes
    }

    fn raw_segment(address: u32, alignment: u32, permissions: u8, data: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&address.to_be_bytes());
        bytes.extend_from_slice(&(data.len() as u64).to_be_bytes());
        bytes.extend_from_slice(&alignment.to_be_bytes());
        bytes.extend_from_slice(&[permissions, 0]);
        bytes.extend_from_slice(&0u16.to_be_bytes());
        bytes.extend_from_slice(data);
        bytes
    }

    #[test]
    fn reads_segmented_program_image() {
        let path = std::env::temp_dir().join(format!("sc16-fixture-{}.img", std::process::id()));
        let bytes = raw_image(
            0x10,
            &[
                raw_segment(0x10, 1, PERMISSION_MASK, &[0x12, 0x34]),
                raw_segment(0x100, 1, PERMISSION_READ, &[0xab]),
            ],
        );
        fs::write(&path, bytes).unwrap();
        let parsed = read_image(&path).unwrap();
        fs::remove_file(path).unwrap();
        assert_eq!(parsed.entry_point, 0x10);
        assert_eq!(
            parsed.segments,
            vec![
                ImageSegment::new(0x10, vec![0x12, 0x34]),
                ImageSegment {
                    address: 0x100,
                    data: vec![0xab],
                    alignment: 1,
                    permissions: SegmentPermissions {
                        read: true,
                        write: false,
                        execute: false,
                    },
                }
            ]
        );
    }

    #[test]
    fn rejects_entry_point_outside_load_segments() {
        let path = std::env::temp_dir().join(format!(
            "sc16-invalid-entry-image-{}.img",
            std::process::id()
        ));
        let bytes = raw_image(
            0x20,
            &[raw_segment(0x10, 1, PERMISSION_MASK, &[0x12, 0x34])],
        );
        fs::write(&path, bytes).unwrap();

        let result = read_image(&path);

        fs::remove_file(path).unwrap();
        assert!(matches!(result, Err(ImageError::InvalidFormat(_))));

        let path =
            std::env::temp_dir().join(format!("sc16-adjacent-segments-{}.img", std::process::id()));
        let adjacent = ProgramImage {
            entry_point: 0x10,
            segments: vec![
                ImageSegment::new(0x10, vec![0x12, 0x34]),
                ImageSegment::new(0x12, vec![0x56]),
            ],
        };
        write_image(&path, &adjacent).unwrap();
        assert_eq!(read_image(&path).unwrap(), adjacent);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_overlapping_load_segments() {
        let path = std::env::temp_dir().join(format!(
            "sc16-overlapping-segments-{}.img",
            std::process::id()
        ));
        let image = ProgramImage {
            entry_point: 0x10,
            segments: vec![
                ImageSegment::new(0x10, vec![0x12, 0x34]),
                ImageSegment::new(0x11, vec![0x56]),
            ],
        };
        assert!(matches!(
            write_image(&path, &image),
            Err(ImageError::InvalidFormat(_))
        ));

        let bytes = raw_image(
            0x10,
            &[
                raw_segment(0x10, 1, PERMISSION_MASK, &[0x12, 0x34]),
                raw_segment(0x11, 1, PERMISSION_MASK, &[0x56]),
            ],
        );
        fs::write(&path, bytes).unwrap();

        let result = read_image(&path);

        fs::remove_file(path).unwrap();
        assert!(matches!(result, Err(ImageError::InvalidFormat(_))));
    }

    #[test]
    fn writes_and_reads_segmented_program_images() {
        let path = std::env::temp_dir().join(format!("sc16-roundtrip-{}.img", std::process::id()));
        let image = ProgramImage {
            entry_point: 0x100,
            segments: vec![
                ImageSegment::new(0x100, vec![0x12, 0x34]),
                ImageSegment {
                    address: 0x20_000,
                    data: vec![0xab],
                    alignment: 0x1000,
                    permissions: SegmentPermissions {
                        read: true,
                        write: false,
                        execute: false,
                    },
                },
            ],
        };
        write_image(&path, &image).unwrap();
        assert_eq!(read_image(&path).unwrap(), image);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_replaced_v1_images_and_unknown_versions() {
        let path =
            std::env::temp_dir().join(format!("sc16-old-image-version-{}.img", std::process::id()));
        let mut bytes = raw_image(0, &[raw_segment(0, 1, PERMISSION_MASK, &[0, 0])]);
        bytes[..8].copy_from_slice(PREVIOUS_MAGIC);
        fs::write(&path, bytes).unwrap();
        assert!(matches!(
            read_image(&path),
            Err(ImageError::InvalidFormat("unsupported image version"))
        ));

        let mut bytes = raw_image(0, &[raw_segment(0, 1, PERMISSION_MASK, &[0, 0])]);
        bytes[9] = 3;
        fs::write(&path, bytes).unwrap();
        assert!(matches!(
            read_image(&path),
            Err(ImageError::InvalidFormat("unsupported image version"))
        ));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_malformed_header_and_segment_metadata() {
        let path =
            std::env::temp_dir().join(format!("sc16-malformed-image-{}.img", std::process::id()));
        let valid_segment = raw_segment(0, 1, PERMISSION_MASK, &[0, 0]);

        let mut bytes = raw_image(0, &[valid_segment.clone()]);
        bytes[11] = 1;
        fs::write(&path, bytes).unwrap();
        assert!(matches!(
            read_image(&path),
            Err(ImageError::InvalidFormat("nonzero reserved header flags"))
        ));

        let mut reserved = raw_segment(0, 1, PERMISSION_MASK, &[0, 0]);
        reserved[17] = 1;
        fs::write(&path, raw_image(0, &[reserved])).unwrap();
        assert!(matches!(
            read_image(&path),
            Err(ImageError::InvalidFormat("nonzero reserved segment fields"))
        ));

        fs::write(&path, raw_image(0, &[raw_segment(0, 1, 0x80, &[0, 0])])).unwrap();
        assert!(matches!(
            read_image(&path),
            Err(ImageError::InvalidFormat(
                "segment contains unknown permission flags"
            ))
        ));

        fs::write(
            &path,
            raw_image(0, &[raw_segment(0, 1, PERMISSION_READ, &[0, 0])]),
        )
        .unwrap();
        assert!(matches!(
            read_image(&path),
            Err(ImageError::InvalidFormat(
                "entry point is not inside an executable load segment"
            ))
        ));

        fs::write(
            &path,
            raw_image(0, &[raw_segment(0, 3, PERMISSION_MASK, &[0, 0])]),
        )
        .unwrap();
        assert!(matches!(
            read_image(&path),
            Err(ImageError::InvalidFormat("invalid segment alignment"))
        ));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_empty_unpermitted_out_of_range_and_excessive_segments() {
        let path =
            std::env::temp_dir().join(format!("sc16-image-limits-{}.img", std::process::id()));
        let mut bytes = raw_image(0, &[raw_segment(0, 1, PERMISSION_MASK, &[])]);
        fs::write(&path, bytes).unwrap();
        assert!(matches!(
            read_image(&path),
            Err(ImageError::InvalidFormat("empty load segment"))
        ));

        bytes = raw_image(0, &[raw_segment(0, 1, 0, &[0, 0])]);
        fs::write(&path, bytes).unwrap();
        assert!(matches!(
            read_image(&path),
            Err(ImageError::InvalidFormat("load segment has no permissions"))
        ));

        bytes = raw_image(
            0,
            &[raw_segment(MEMORY_SIZE as u32, 1, PERMISSION_MASK, &[0])],
        );
        fs::write(&path, bytes).unwrap();
        let result = read_image(&path);
        assert!(
            matches!(
                &result,
                Err(ImageError::InvalidFormat("segment is outside SC-16 memory"))
            ),
            "{result:?}"
        );

        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&FORMAT_VERSION.to_be_bytes());
        bytes.extend_from_slice(&0u16.to_be_bytes());
        bytes.extend_from_slice(&0u32.to_be_bytes());
        bytes.extend_from_slice(&((MAX_SEGMENTS as u32) + 1).to_be_bytes());
        fs::write(&path, bytes).unwrap();
        assert!(matches!(
            read_image(&path),
            Err(ImageError::InvalidFormat("segment count is excessive"))
        ));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_truncated_and_trailing_image_bytes() {
        let path =
            std::env::temp_dir().join(format!("sc16-truncated-image-{}.img", std::process::id()));
        let valid = raw_image(0, &[raw_segment(0, 1, PERMISSION_MASK, &[0, 0])]);
        fs::write(&path, &valid[..valid.len() - 1]).unwrap();
        assert!(matches!(
            read_image(&path),
            Err(ImageError::InvalidFormat("truncated image"))
                | Err(ImageError::InvalidFormat("truncated segment data"))
        ));

        let mut trailing = valid;
        trailing.push(0);
        fs::write(&path, trailing).unwrap();
        assert!(matches!(
            read_image(&path),
            Err(ImageError::InvalidFormat("trailing bytes"))
        ));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn writer_rejects_invalid_alignment_and_non_executable_entry() {
        let path = std::env::temp_dir().join(format!(
            "sc16-invalid-image-write-{}.img",
            std::process::id()
        ));
        let mut image = ProgramImage {
            entry_point: 0,
            segments: vec![ImageSegment::new(0, vec![0, 0])],
        };
        image.segments[0].alignment = 3;
        assert!(matches!(
            write_image(&path, &image),
            Err(ImageError::InvalidFormat("invalid segment alignment"))
        ));

        image.segments[0] = ImageSegment {
            address: 0,
            data: vec![0, 0],
            alignment: 1,
            permissions: SegmentPermissions {
                read: true,
                write: true,
                execute: false,
            },
        };
        assert!(matches!(
            write_image(&path, &image),
            Err(ImageError::InvalidFormat(
                "entry point is not inside an executable load segment"
            ))
        ));
    }

    #[test]
    fn builds_assembly_sources_natively() {
        let path =
            std::env::temp_dir().join(format!("sc16-assembly-source-{}.asm", std::process::id()));
        fs::write(&path, "start: LDI R0, 1\nHALT\n").unwrap();
        let image = build_source(&path).unwrap();
        fs::remove_file(path).unwrap();
        assert_eq!(image.entry_point, 0);
        assert_eq!(image.segments[0].data, b"\x08\x01\x00\x00");
    }

    #[test]
    fn writes_version_two_image_and_executes_loaded_entry() {
        let source_path =
            std::env::temp_dir().join(format!("sc16-image-entry-{}.asm", std::process::id()));
        let image_path =
            std::env::temp_dir().join(format!("sc16-image-entry-{}.img", std::process::id()));
        fs::write(&source_path, "LDI R0, 42\nHALT\n").unwrap();
        let image = build_source(&source_path).unwrap();
        write_image(&image_path, &image).unwrap();
        assert_eq!(&fs::read(&image_path).unwrap()[..8], MAGIC);
        let loaded = read_image(&image_path).unwrap();
        fs::remove_file(source_path).unwrap();
        fs::remove_file(image_path).unwrap();

        assert_eq!(
            loaded.segments[0].permissions,
            SegmentPermissions::READ_WRITE_EXECUTE
        );
        assert_eq!(loaded.segments[0].alignment, 1);
        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&loaded.segments, loaded.entry_point)
            .unwrap();
        assert_eq!(
            cpu.loaded_image_segments,
            [crate::cpu::LoadedSegmentMetadata {
                address: 0,
                length: 4,
                alignment: 1,
                permissions: SegmentPermissions::READ_WRITE_EXECUTE,
            }]
        );
        cpu.run(16).unwrap();
        assert_eq!(cpu.registers[0], 42);
    }

    #[test]
    fn loads_binary_programs_and_rejects_json_data_files() {
        let binary_path =
            std::env::temp_dir().join(format!("sc16-binary-{}.bin", std::process::id()));
        fs::write(&binary_path, [0x12, 0x34, 0x56]).unwrap();
        let image = load_program(&binary_path).unwrap();
        fs::remove_file(binary_path).unwrap();
        assert_eq!(image.entry_point, 0);
        assert_eq!(
            image.segments,
            [ImageSegment::new(0, vec![0x12, 0x34, 0x56])]
        );

        let container_path =
            std::env::temp_dir().join(format!("sc16-container-{}.bin", std::process::id()));
        let container = ProgramImage {
            entry_point: 0x100,
            segments: vec![ImageSegment::new(0x100, vec![0, 0])],
        };
        write_image(&container_path, &container).unwrap();
        assert_eq!(load_program(&container_path).unwrap(), container);
        fs::remove_file(&container_path).unwrap();

        let named_image_path =
            std::env::temp_dir().join(format!("sc16-named-image-{}.sc16img", std::process::id()));
        write_image(&named_image_path, &container).unwrap();
        assert_eq!(load_program(&named_image_path).unwrap(), container);
        fs::remove_file(&named_image_path).unwrap();

        let legacy_path =
            std::env::temp_dir().join(format!("sc16-legacy-container-{}.bin", std::process::id()));
        fs::write(&legacy_path, [PREVIOUS_MAGIC.as_slice(), &[0; 16]].concat()).unwrap();
        assert!(matches!(
            load_program(&legacy_path),
            Err(ImageError::InvalidFormat("unsupported image version"))
        ));
        fs::remove_file(&legacy_path).unwrap();

        let json_path = std::env::temp_dir().join(format!("sc16-data-{}.json", std::process::id()));
        fs::write(&json_path, b"{}").unwrap();
        let error = load_program(&json_path).unwrap_err().to_string();
        fs::remove_file(json_path).unwrap();
        assert!(error.contains("JSON files are data; load them through the disk API"));
    }

    #[test]
    fn builds_supported_c_sources_natively() {
        let path = std::env::temp_dir().join(format!("sc16-native-c-{}.c", std::process::id()));
        fs::write(
            &path,
            "int add(int left, int right) { return left + right; } \
             int main(void) { return add(40, 2); }\n",
        )
        .unwrap();
        let image = build_source(&path).unwrap();
        fs::remove_file(path).unwrap();

        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(256).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 42);
    }

    #[test]
    fn builds_and_runs_factorial_example_with_the_rust_compiler() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join("factorial.c");
        let image = build_source(&path).unwrap();
        assert_eq!(image.entry_point, 0);
        assert!(!image.segments.is_empty());

        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(1024).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 0);
        assert_eq!(cpu.read_u16(0x80000), 123);
    }

    #[test]
    fn rejects_unsupported_c_without_a_fallback_compiler() {
        let path = std::env::temp_dir().join(format!("sc16-unsupported-{}.c", std::process::id()));
        fs::write(&path, "int main(void) { return calculate_answer(); }").unwrap();
        let result = build_source(&path);
        fs::remove_file(path).unwrap();
        assert!(matches!(result, Err(ImageError::UnsupportedC(_))));
    }

    #[test]
    fn reports_c_parse_errors_as_compiler_diagnostics() {
        let path = std::env::temp_dir().join(format!("sc16-invalid-c-{}.c", std::process::id()));
        fs::write(&path, "int main(void) { return ;").unwrap();
        let result = build_source(&path);
        fs::remove_file(&path).unwrap();
        let error = result.unwrap_err().to_string();
        assert!(error.contains("Rust C compiler error:"));
        assert!(error.contains("unterminated compound statement"));
    }
}
