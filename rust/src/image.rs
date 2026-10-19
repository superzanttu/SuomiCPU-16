use crate::{CpuError, MemorySegment};
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

const MAGIC: &[u8; 8] = b"SC16IMG1";
const MEMORY_SIZE: u64 = 1 << 30;

#[derive(Debug)]
pub enum ImageError {
    Io(io::Error),
    UnsupportedC(String),
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
    pub segments: Vec<MemorySegment>,
}

pub fn load_binary(path: &Path) -> Result<ProgramImage, ImageError> {
    let data = fs::read(path)?;
    if data.len() as u64 > MEMORY_SIZE {
        return Err(ImageError::InvalidFormat("binary exceeds SC-16 memory"));
    }
    Ok(ProgramImage {
        entry_point: 0,
        segments: vec![MemorySegment { address: 0, data }],
    })
}

pub fn write_image(path: &Path, image: &ProgramImage) -> Result<(), ImageError> {
    if image.entry_point >= MEMORY_SIZE as u32 {
        return Err(ImageError::InvalidFormat(
            "entry point is outside SC-16 memory",
        ));
    }
    let segment_count = u32::try_from(image.segments.len())
        .map_err(|_| ImageError::InvalidFormat("segment count is excessive"))?;
    let mut total = 0u64;
    for segment in &image.segments {
        let end = u64::from(segment.address)
            .checked_add(segment.data.len() as u64)
            .ok_or(ImageError::InvalidFormat("segment range overflow"))?;
        if end > MEMORY_SIZE {
            return Err(ImageError::InvalidFormat("segment is outside SC-16 memory"));
        }
        total = total
            .checked_add(segment.data.len() as u64)
            .ok_or(ImageError::InvalidFormat("total image size overflow"))?;
        if total > MEMORY_SIZE {
            return Err(ImageError::InvalidFormat("image exceeds memory"));
        }
    }
    let mut file = fs::File::create(path)?;
    file.write_all(MAGIC)?;
    file.write_all(&image.entry_point.to_be_bytes())?;
    file.write_all(&segment_count.to_be_bytes())?;
    for segment in &image.segments {
        file.write_all(&segment.address.to_be_bytes())?;
        file.write_all(&(segment.data.len() as u64).to_be_bytes())?;
        file.write_all(&segment.data)?;
    }
    Ok(())
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
                .map_err(|error| ImageError::Assembly(error.to_string()))?
        {
            return Ok(image);
        }
        return Err(ImageError::UnsupportedC(source.display().to_string()));
    }
    Err(ImageError::InvalidFormat(
        "source must be Rust-supported C, assembly, or flat binary",
    ))
}

pub fn read_image(path: &Path) -> Result<ProgramImage, ImageError> {
    let mut file = fs::File::open(path)?;
    let mut header = [0u8; 16];
    file.read_exact(&mut header)?;
    if &header[..8] != MAGIC {
        return Err(ImageError::InvalidFormat("bad magic"));
    }
    let entry_point = u32::from_be_bytes(header[8..12].try_into().unwrap());
    let segment_count = u32::from_be_bytes(header[12..16].try_into().unwrap());
    if u64::from(segment_count) > MEMORY_SIZE / 12 {
        return Err(ImageError::InvalidFormat("segment count is excessive"));
    }
    let mut segments = Vec::with_capacity(segment_count as usize);
    let mut total = 0u64;
    for _ in 0..segment_count {
        let mut segment_header = [0u8; 12];
        file.read_exact(&mut segment_header)?;
        let address = u32::from_be_bytes(segment_header[..4].try_into().unwrap());
        let length = u64::from_be_bytes(segment_header[4..].try_into().unwrap());
        let end = u64::from(address)
            .checked_add(length)
            .ok_or(ImageError::InvalidFormat("segment range overflow"))?;
        if end > MEMORY_SIZE || length > MEMORY_SIZE {
            return Err(ImageError::InvalidFormat("segment is outside memory"));
        }
        total = total
            .checked_add(length)
            .ok_or(ImageError::InvalidFormat("total image size overflow"))?;
        if total > MEMORY_SIZE {
            return Err(ImageError::InvalidFormat("image exceeds memory"));
        }
        let mut data = vec![0; length as usize];
        file.read_exact(&mut data)?;
        segments.push(MemorySegment { address, data });
    }
    let mut trailing = [0; 1];
    if file.read(&mut trailing)? != 0 {
        return Err(ImageError::InvalidFormat("trailing bytes"));
    }
    Ok(ProgramImage {
        entry_point,
        segments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_segmented_program_image() {
        let path = std::env::temp_dir().join(format!("sc16-fixture-{}.img", std::process::id()));
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&0x10u32.to_be_bytes());
        bytes.extend_from_slice(&2u32.to_be_bytes());
        bytes.extend_from_slice(&0x10u32.to_be_bytes());
        bytes.extend_from_slice(&2u64.to_be_bytes());
        bytes.extend_from_slice(&[0x12, 0x34]);
        bytes.extend_from_slice(&0x100u32.to_be_bytes());
        bytes.extend_from_slice(&1u64.to_be_bytes());
        bytes.push(0xab);
        fs::write(&path, bytes).unwrap();
        let parsed = read_image(&path).unwrap();
        fs::remove_file(path).unwrap();
        assert_eq!(parsed.entry_point, 0x10);
        assert_eq!(
            parsed.segments,
            vec![
                MemorySegment {
                    address: 0x10,
                    data: vec![0x12, 0x34],
                },
                MemorySegment {
                    address: 0x100,
                    data: vec![0xab],
                },
            ]
        );
    }

    #[test]
    fn writes_and_reads_segmented_program_images() {
        let path = std::env::temp_dir().join(format!("sc16-roundtrip-{}.img", std::process::id()));
        let image = ProgramImage {
            entry_point: 0x100,
            segments: vec![
                MemorySegment {
                    address: 0x100,
                    data: vec![0x12, 0x34],
                },
                MemorySegment {
                    address: 0x20_000,
                    data: vec![0xab],
                },
            ],
        };
        write_image(&path, &image).unwrap();
        assert_eq!(read_image(&path).unwrap(), image);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn builds_assembly_sources_without_invoking_python() {
        let path =
            std::env::temp_dir().join(format!("sc16-assembly-source-{}.asm", std::process::id()));
        fs::write(&path, "start: LDI R0, 1\nHALT\n").unwrap();
        let image = build_source(&path).unwrap();
        fs::remove_file(path).unwrap();
        assert_eq!(image.entry_point, 0);
        assert_eq!(image.segments[0].data, b"\x08\x01\x00\x00");
    }

    #[test]
    fn builds_rust_supported_c_sources_without_invoking_python() {
        let path = std::env::temp_dir().join(format!("sc16-native-c-{}.c", std::process::id()));
        fs::write(
            &path,
            "int main(void) { int answer = 40; return answer + 2; }\n",
        )
        .unwrap();
        let image = build_source(&path).unwrap();
        fs::remove_file(path).unwrap();

        let mut cpu = crate::cpu::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(32).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 42);
    }

    #[test]
    fn rejects_unsupported_c_without_starting_a_python_compiler() {
        let path = std::env::temp_dir().join(format!("sc16-unsupported-{}.c", std::process::id()));
        fs::write(&path, "int main(void) { return calculate_answer(); }").unwrap();
        let result = build_source(&path);
        fs::remove_file(path).unwrap();
        assert!(matches!(result, Err(ImageError::UnsupportedC(_))));
    }
}
