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
    pub segments: Vec<MemorySegment>,
}

fn validate_image(image: &ProgramImage) -> Result<(), ImageError> {
    if u64::from(image.entry_point) >= MEMORY_SIZE {
        return Err(ImageError::InvalidFormat(
            "entry point is outside SC-16 memory",
        ));
    }
    let mut total = 0u64;
    let mut contains_entry_point = false;
    let mut ranges = Vec::with_capacity(image.segments.len());
    for segment in &image.segments {
        let length = segment.data.len() as u64;
        let start = u64::from(segment.address);
        let end = start
            .checked_add(length)
            .ok_or(ImageError::InvalidFormat("segment range overflow"))?;
        if end > MEMORY_SIZE {
            return Err(ImageError::InvalidFormat("segment is outside SC-16 memory"));
        }
        if length > 0
            && u64::from(segment.address) <= u64::from(image.entry_point)
            && u64::from(image.entry_point) < end
        {
            contains_entry_point = true;
        }
        if length > 0 {
            ranges.push((start, end));
        }
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
    if !contains_entry_point {
        return Err(ImageError::InvalidFormat(
            "entry point is not inside a load segment",
        ));
    }
    Ok(())
}

pub fn load_binary(path: &Path) -> Result<ProgramImage, ImageError> {
    let data = fs::read(path)?;
    if data.len() as u64 > MEMORY_SIZE {
        return Err(ImageError::InvalidFormat("binary exceeds SC-16 memory"));
    }
    let image = ProgramImage {
        entry_point: 0,
        segments: vec![MemorySegment { address: 0, data }],
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

pub fn write_image(path: &Path, image: &ProgramImage) -> Result<(), ImageError> {
    validate_image(image)?;
    let segment_count = u32::try_from(image.segments.len())
        .map_err(|_| ImageError::InvalidFormat("segment count is excessive"))?;
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
    fn rejects_entry_point_outside_load_segments() {
        let path = std::env::temp_dir().join(format!(
            "sc16-invalid-entry-image-{}.img",
            std::process::id()
        ));
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&0x20u32.to_be_bytes());
        bytes.extend_from_slice(&1u32.to_be_bytes());
        bytes.extend_from_slice(&0x10u32.to_be_bytes());
        bytes.extend_from_slice(&2u64.to_be_bytes());
        bytes.extend_from_slice(&[0x12, 0x34]);
        fs::write(&path, bytes).unwrap();

        let result = read_image(&path);

        fs::remove_file(path).unwrap();
        assert!(matches!(result, Err(ImageError::InvalidFormat(_))));

        let path =
            std::env::temp_dir().join(format!("sc16-adjacent-segments-{}.img", std::process::id()));
        let adjacent = ProgramImage {
            entry_point: 0x10,
            segments: vec![
                MemorySegment {
                    address: 0x10,
                    data: vec![0x12, 0x34],
                },
                MemorySegment {
                    address: 0x12,
                    data: vec![0x56],
                },
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
                MemorySegment {
                    address: 0x10,
                    data: vec![0x12, 0x34],
                },
                MemorySegment {
                    address: 0x11,
                    data: vec![0x56],
                },
            ],
        };
        assert!(matches!(
            write_image(&path, &image),
            Err(ImageError::InvalidFormat(_))
        ));

        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&0x10u32.to_be_bytes());
        bytes.extend_from_slice(&2u32.to_be_bytes());
        bytes.extend_from_slice(&0x10u32.to_be_bytes());
        bytes.extend_from_slice(&2u64.to_be_bytes());
        bytes.extend_from_slice(&[0x12, 0x34]);
        bytes.extend_from_slice(&0x11u32.to_be_bytes());
        bytes.extend_from_slice(&1u64.to_be_bytes());
        bytes.push(0x56);
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
    fn loads_binary_programs_and_rejects_json_data_files() {
        let binary_path =
            std::env::temp_dir().join(format!("sc16-binary-{}.bin", std::process::id()));
        fs::write(&binary_path, [0x12, 0x34, 0x56]).unwrap();
        let image = load_program(&binary_path).unwrap();
        fs::remove_file(binary_path).unwrap();
        assert_eq!(image.entry_point, 0);
        assert_eq!(
            image.segments,
            [MemorySegment {
                address: 0,
                data: vec![0x12, 0x34, 0x56],
            }]
        );

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
