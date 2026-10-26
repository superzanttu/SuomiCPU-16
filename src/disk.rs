use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};

const BLOCK_SIZE: usize = 256;
const MAX_FILE_READ_SIZE: usize = 4096;
const FOLDER_DISK_MANIFEST_VERSION: u64 = 1;
const MAX_DISK_LABEL_LENGTH: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderDiskManifest {
    pub version: u32,
    pub id: String,
    pub label: String,
    pub capacity_bytes: u64,
    pub read_only: bool,
    pub protected_files: Vec<String>,
}

pub fn parse_folder_disk_manifest(raw: &str) -> Result<FolderDiskManifest, String> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|error| format!("invalid folder disk manifest: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "folder disk manifest must be a JSON object".to_owned())?;
    for key in object.keys() {
        if !matches!(
            key.as_str(),
            "version" | "id" | "label" | "capacity_bytes" | "read_only" | "protected_files"
        ) {
            return Err(format!("unknown folder disk manifest field {key:?}"));
        }
    }
    let version = object
        .get("version")
        .and_then(Value::as_u64)
        .filter(|version| *version == FOLDER_DISK_MANIFEST_VERSION)
        .ok_or_else(|| "unsupported or missing folder disk manifest version".to_owned())?;
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "folder disk manifest requires a string id".to_owned())?;
    validate_disk_id(id)?;
    let label = object
        .get("label")
        .and_then(Value::as_str)
        .filter(|label| {
            !label.trim().is_empty()
                && label.chars().count() <= MAX_DISK_LABEL_LENGTH
                && !label.chars().any(char::is_control)
        })
        .ok_or_else(|| "folder disk manifest requires a valid label".to_owned())?;
    let capacity_bytes = object
        .get("capacity_bytes")
        .and_then(Value::as_u64)
        .filter(|capacity| *capacity > 0)
        .ok_or_else(|| "folder disk manifest requires positive capacity_bytes".to_owned())?;
    let read_only = object
        .get("read_only")
        .and_then(Value::as_bool)
        .ok_or_else(|| "folder disk manifest requires a boolean read_only".to_owned())?;
    let protected_files = object
        .get("protected_files")
        .map(|value| {
            let files = value
                .as_array()
                .ok_or_else(|| "protected_files must be an array of paths".to_owned())?;
            let mut protected_files = Vec::with_capacity(files.len());
            for file in files {
                let file = file
                    .as_str()
                    .ok_or_else(|| "protected_files must contain only strings".to_owned())?;
                validate_file_path(file)?;
                if protected_files.iter().any(|existing| existing == file) {
                    return Err(format!("duplicate protected file path {file:?}"));
                }
                protected_files.push(file.to_owned());
            }
            Ok(protected_files)
        })
        .transpose()?
        .unwrap_or_default();
    Ok(FolderDiskManifest {
        version: version as u32,
        id: id.to_owned(),
        label: label.to_owned(),
        capacity_bytes,
        read_only,
        protected_files,
    })
}

pub struct FolderDisk {
    root: PathBuf,
    manifest: FolderDiskManifest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderBootMode {
    Kernel,
    User,
}

#[derive(Debug)]
pub struct FolderBootImage {
    pub mode: FolderBootMode,
    pub image: crate::image::ProgramImage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderDirectoryEntry {
    pub name: String,
    pub is_directory: bool,
    pub size_bytes: u64,
}

impl FolderDisk {
    pub fn open(root: &Path) -> Result<Self, String> {
        let root_metadata = fs::symlink_metadata(root)
            .map_err(|error| format!("cannot inspect folder disk root: {error}"))?;
        if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
            return Err("folder disk root must be a real directory".to_owned());
        }
        let root = fs::canonicalize(root)
            .map_err(|error| format!("cannot resolve folder disk root: {error}"))?;
        let directory_id = root
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "folder disk directory name must be a valid disk ID".to_owned())?;
        validate_disk_id(directory_id)?;

        let manifest_path = root.join("disk.json");
        let manifest_metadata = fs::symlink_metadata(&manifest_path)
            .map_err(|error| format!("cannot inspect folder disk manifest: {error}"))?;
        if manifest_metadata.file_type().is_symlink() || !manifest_metadata.is_file() {
            return Err("folder disk manifest must be a regular file".to_owned());
        }
        let manifest = parse_folder_disk_manifest(
            &fs::read_to_string(&manifest_path)
                .map_err(|error| format!("cannot read folder disk manifest: {error}"))?,
        )?;
        if manifest.id != directory_id {
            return Err("folder disk manifest ID does not match directory name".to_owned());
        }
        let disk = Self { root, manifest };
        if disk.used_bytes()? > disk.manifest.capacity_bytes {
            return Err("folder disk usage exceeds declared capacity".to_owned());
        }
        Ok(disk)
    }

    pub fn discover(root: &Path) -> Result<Vec<Self>, String> {
        Self::discover_paths(root)?
            .iter()
            .map(|path| Self::open(path))
            .collect()
    }

    pub fn discover_paths(root: &Path) -> Result<Vec<PathBuf>, String> {
        let root_metadata = fs::symlink_metadata(root)
            .map_err(|error| format!("cannot inspect folder disk collection: {error}"))?;
        if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
            return Err("folder disk collection must be a real directory".to_owned());
        }
        let mut candidates = Vec::new();
        for entry in
            fs::read_dir(root).map_err(|error| format!("cannot enumerate folder disks: {error}"))?
        {
            let entry = entry.map_err(|error| format!("cannot enumerate folder disks: {error}"))?;
            if !entry
                .file_type()
                .map_err(|error| format!("cannot inspect folder disk entry: {error}"))?
                .is_dir()
            {
                continue;
            }
            let path = entry.path();
            let Some(id) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if valid_disk_id(id) {
                candidates.push(path);
            }
        }
        candidates.sort_by_key(|path| path.file_name().map(|name| name.to_os_string()));
        Ok(candidates)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn manifest(&self) -> &FolderDiskManifest {
        &self.manifest
    }

    pub fn list_dir(&self, directory: Option<&str>) -> Result<Vec<FolderDirectoryEntry>, String> {
        let directory = self.resolve_existing_directory(directory)?;
        let mut entries = Vec::new();
        for entry in fs::read_dir(&directory)
            .map_err(|error| format!("cannot list folder disk directory: {error}"))?
        {
            let entry =
                entry.map_err(|error| format!("cannot list folder disk directory: {error}"))?;
            let path = entry.path();
            if directory == self.root && entry.file_name() == "disk.json" {
                continue;
            }
            let file_type = entry
                .file_type()
                .map_err(|error| format!("cannot inspect folder disk entry: {error}"))?;
            if file_type.is_symlink() || (!file_type.is_dir() && !file_type.is_file()) {
                return Err(format!("folder disk contains unsupported entry {path:?}"));
            }
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| "folder disk entry name must be ASCII".to_owned())?;
            validate_file_path(&name)?;
            let size_bytes = if file_type.is_file() {
                entry
                    .metadata()
                    .map_err(|error| format!("cannot inspect folder disk file: {error}"))?
                    .len()
            } else {
                0
            };
            entries.push(FolderDirectoryEntry {
                name,
                is_directory: file_type.is_dir(),
                size_bytes,
            });
        }
        entries.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(entries)
    }

    pub fn load_boot_image(&self) -> Result<FolderBootImage, String> {
        let main_path = self.root.join("main.bin");
        let kernel_path = self.root.join("kernel.bin");
        let main_exists = self.is_regular_file(&main_path, "main.bin")?;
        let kernel_exists = self.is_regular_file(&kernel_path, "kernel.bin")?;
        let (path, mode) = match (main_exists, kernel_exists) {
            (true, false) => (main_path, FolderBootMode::User),
            (false, true) => (kernel_path, FolderBootMode::Kernel),
            (true, true) => return Err("NOT_BOOT: both main.bin and kernel.bin exist".to_owned()),
            (false, false) => return Err("NOT_BOOT: no boot executable exists".to_owned()),
        };
        let image = crate::image::read_image(&path)
            .map_err(|error| format!("invalid boot image {}: {error}", path.display()))?;
        Ok(FolderBootImage { mode, image })
    }

    fn is_regular_file(&self, path: &Path, name: &str) -> Result<bool, String> {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                Err(format!("boot executable {name} must be a regular file"))
            }
            Ok(_) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(format!("cannot inspect boot executable {name}: {error}")),
        }
    }

    pub fn list_files(&self) -> Result<Vec<String>, String> {
        let mut files = Vec::new();
        collect_folder_files(&self.root, &self.root, &mut files)?;
        files.sort_by(|left, right| left.0.cmp(&right.0));
        Ok(files.into_iter().map(|(path, _)| path).collect())
    }

    pub fn used_bytes(&self) -> Result<u64, String> {
        let mut files = Vec::new();
        collect_folder_files(&self.root, &self.root, &mut files)?;
        files.into_iter().try_fold(0u64, |total, (_, length)| {
            total
                .checked_add(length)
                .ok_or_else(|| "folder disk usage overflow".to_owned())
        })
    }

    pub fn read_file(
        &self,
        file_path: &str,
        offset: u64,
        length: usize,
    ) -> Result<(Vec<u8>, u64), String> {
        if length > MAX_FILE_READ_SIZE {
            return Err("invalid virtual file read range".to_owned());
        }
        let path = self.resolve_regular_file(file_path)?;
        let mut file = File::open(&path)
            .map_err(|error| format!("cannot read folder disk file {file_path:?}: {error}"))?;
        let file_size = file
            .metadata()
            .map_err(|error| format!("cannot inspect folder disk file {file_path:?}: {error}"))?
            .len();
        file.seek(SeekFrom::Start(offset))
            .map_err(|error| format!("cannot seek folder disk file {file_path:?}: {error}"))?;
        let mut data = vec![0; length];
        let count = file
            .read(&mut data)
            .map_err(|error| format!("cannot read folder disk file {file_path:?}: {error}"))?;
        data.truncate(count);
        Ok((data, file_size))
    }

    pub fn write_file(&self, file_path: &str, data: &[u8]) -> Result<(), String> {
        let relative = validate_file_path(file_path)?;
        self.ensure_mutable(file_path)?;
        let parent = self.resolve_parent_directory(&relative)?;
        let target = parent.join(
            relative
                .file_name()
                .ok_or_else(|| "invalid folder disk file path".to_owned())?,
        );
        let previous_length = match fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err("folder disk files must be regular files".to_owned());
            }
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => return Err(format!("cannot inspect folder disk file: {error}")),
        };
        let used_bytes = self.used_bytes()?;
        let new_usage = used_bytes
            .checked_sub(previous_length)
            .and_then(|usage| usage.checked_add(data.len() as u64))
            .ok_or_else(|| "folder disk usage overflow".to_owned())?;
        if new_usage > self.manifest.capacity_bytes {
            return Err("folder disk capacity exceeded".to_owned());
        }
        fs::write(&target, data)
            .map_err(|error| format!("cannot write folder disk file {file_path:?}: {error}"))
    }

    pub fn copy_file(&self, source: &str, destination: &str) -> Result<(), String> {
        let source_path = self.resolve_regular_file(source)?;
        let destination_relative = validate_file_path(destination)?;
        self.ensure_mutable(destination)?;
        let destination_parent = self.resolve_parent_directory(&destination_relative)?;
        let destination_path = destination_parent.join(
            destination_relative
                .file_name()
                .ok_or_else(|| "invalid folder disk file path".to_owned())?,
        );
        match fs::symlink_metadata(&destination_path) {
            Ok(_) => return Err("folder disk copy destination already exists".to_owned()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("cannot inspect copy destination: {error}")),
        }
        let source_length = fs::metadata(&source_path)
            .map_err(|error| format!("cannot inspect copy source {source:?}: {error}"))?
            .len();
        let new_usage = self
            .used_bytes()?
            .checked_add(source_length)
            .ok_or_else(|| "folder disk usage overflow".to_owned())?;
        if new_usage > self.manifest.capacity_bytes {
            return Err("folder disk capacity exceeded".to_owned());
        }

        let mut source_file = File::open(&source_path)
            .map_err(|error| format!("cannot open copy source {source:?}: {error}"))?;
        let mut destination_file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination_path)
            .map_err(|error| format!("cannot create copy destination {destination:?}: {error}"))?;
        if let Err(error) = std::io::copy(&mut source_file, &mut destination_file) {
            drop(destination_file);
            let _ = fs::remove_file(&destination_path);
            return Err(format!("cannot copy folder disk file {source:?}: {error}"));
        }
        Ok(())
    }

    pub fn create_dir(&self, directory: &str) -> Result<(), String> {
        let relative = validate_file_path(directory)?;
        self.ensure_mutable(directory)?;
        let parent = self.resolve_parent_directory(&relative)?;
        let target = parent.join(
            relative
                .file_name()
                .ok_or_else(|| "invalid folder disk directory path".to_owned())?,
        );
        fs::create_dir(target)
            .map_err(|error| format!("cannot create folder disk directory {directory:?}: {error}"))
    }

    pub fn delete_file(&self, file_path: &str) -> Result<(), String> {
        self.ensure_mutable(file_path)?;
        let path = self.resolve_regular_file(file_path)?;
        fs::remove_file(&path)
            .map_err(|error| format!("cannot delete folder disk file {file_path:?}: {error}"))
    }

    pub fn delete_entry(&self, path: &str) -> Result<(), String> {
        self.ensure_mutable(path)?;
        let relative = validate_file_path(path)?;
        let (target, metadata) = self.resolve_entry(&relative)?;
        if metadata.is_dir() {
            if self.has_protected_descendant(path) {
                return Err(format!(
                    "folder disk directory {path:?} contains protected files"
                ));
            }
            fs::remove_dir(&target)
                .map_err(|error| format!("cannot delete folder disk directory {path:?}: {error}"))
        } else if metadata.is_file() {
            fs::remove_file(&target)
                .map_err(|error| format!("cannot delete folder disk file {path:?}: {error}"))
        } else {
            Err("folder disk entries must be regular files or directories".to_owned())
        }
    }

    pub fn rename_file(&self, source: &str, destination: &str) -> Result<(), String> {
        self.ensure_mutable(source)?;
        self.ensure_mutable(destination)?;
        let source_path = self.resolve_regular_file(source)?;
        let destination_relative = validate_file_path(destination)?;
        let destination_parent = self.resolve_parent_directory(&destination_relative)?;
        let destination_path = destination_parent.join(
            destination_relative
                .file_name()
                .ok_or_else(|| "invalid folder disk file path".to_owned())?,
        );
        match fs::symlink_metadata(&destination_path) {
            Ok(_) => return Err("folder disk rename destination already exists".to_owned()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("cannot inspect rename destination: {error}")),
        }
        fs::rename(&source_path, &destination_path)
            .map_err(|error| format!("cannot rename folder disk file {source:?}: {error}"))
    }

    pub fn rename_entry(&self, source: &str, destination: &str) -> Result<(), String> {
        self.ensure_mutable(source)?;
        self.ensure_mutable(destination)?;
        let source_relative = validate_file_path(source)?;
        let (source_path, source_metadata) = self.resolve_entry(&source_relative)?;
        if source_metadata.is_dir() && self.has_protected_descendant(source) {
            return Err(format!(
                "folder disk directory {source:?} contains protected files"
            ));
        }
        if !source_metadata.is_dir() && !source_metadata.is_file() {
            return Err("folder disk entries must be regular files or directories".to_owned());
        }
        let destination_relative = validate_file_path(destination)?;
        let destination_parent = self.resolve_parent_directory(&destination_relative)?;
        let destination_path = destination_parent.join(
            destination_relative
                .file_name()
                .ok_or_else(|| "invalid folder disk entry path".to_owned())?,
        );
        match fs::symlink_metadata(&destination_path) {
            Ok(_) => return Err("folder disk rename destination already exists".to_owned()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("cannot inspect rename destination: {error}")),
        }
        fs::rename(&source_path, &destination_path)
            .map_err(|error| format!("cannot rename folder disk entry {source:?}: {error}"))
    }

    fn resolve_regular_file(&self, file_path: &str) -> Result<PathBuf, String> {
        let relative = validate_file_path(file_path)?;
        let parent = self.resolve_parent_directory(&relative)?;
        let path = parent.join(
            relative
                .file_name()
                .ok_or_else(|| "invalid folder disk file path".to_owned())?,
        );
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("cannot inspect folder disk file {file_path:?}: {error}"))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("folder disk files must be regular files".to_owned());
        }
        let resolved = fs::canonicalize(&path)
            .map_err(|error| format!("cannot resolve folder disk file {file_path:?}: {error}"))?;
        if !resolved.starts_with(&self.root) {
            return Err("folder disk path escapes the mounted root".to_owned());
        }
        Ok(resolved)
    }

    fn resolve_parent_directory(&self, relative: &Path) -> Result<PathBuf, String> {
        let mut parent = self.root.clone();
        if let Some(relative_parent) = relative.parent() {
            for component in relative_parent.components() {
                let Component::Normal(name) = component else {
                    return Err("invalid folder disk path".to_owned());
                };
                parent.push(name);
                let metadata = fs::symlink_metadata(&parent)
                    .map_err(|error| format!("cannot inspect folder disk path: {error}"))?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err("folder disk path contains a non-directory component".to_owned());
                }
            }
        }
        let resolved = fs::canonicalize(parent)
            .map_err(|error| format!("cannot resolve folder disk path: {error}"))?;
        if !resolved.starts_with(&self.root) {
            return Err("folder disk path escapes the mounted root".to_owned());
        }
        Ok(resolved)
    }

    fn resolve_existing_directory(&self, directory: Option<&str>) -> Result<PathBuf, String> {
        let Some(directory) = directory else {
            return Ok(self.root.clone());
        };
        let relative = validate_file_path(directory)?;
        let mut current = self.root.clone();
        for component in relative.components() {
            let Component::Normal(name) = component else {
                return Err("invalid folder disk directory path".to_owned());
            };
            current.push(name);
            let metadata = fs::symlink_metadata(&current)
                .map_err(|error| format!("cannot inspect folder disk directory: {error}"))?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err("folder disk path contains a non-directory component".to_owned());
            }
        }
        let resolved = fs::canonicalize(current)
            .map_err(|error| format!("cannot resolve folder disk directory: {error}"))?;
        if !resolved.starts_with(&self.root) {
            return Err("folder disk path escapes the mounted root".to_owned());
        }
        Ok(resolved)
    }

    fn resolve_entry(&self, relative: &Path) -> Result<(PathBuf, fs::Metadata), String> {
        let parent = self.resolve_parent_directory(relative)?;
        let path = parent.join(
            relative
                .file_name()
                .ok_or_else(|| "invalid folder disk entry path".to_owned())?,
        );
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("cannot inspect folder disk entry: {error}"))?;
        if metadata.file_type().is_symlink() {
            return Err("folder disk symlinks are not supported".to_owned());
        }
        Ok((path, metadata))
    }

    fn has_protected_descendant(&self, directory: &str) -> bool {
        self.manifest.protected_files.iter().any(|protected| {
            protected
                .get(..directory.len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(directory))
                && protected
                    .as_bytes()
                    .get(directory.len())
                    .is_some_and(|separator| *separator == b'/')
        })
    }

    fn ensure_mutable(&self, file_path: &str) -> Result<(), String> {
        if self.manifest.read_only {
            return Err("folder disk is read-only".to_owned());
        }
        if file_path.eq_ignore_ascii_case("disk.json")
            || self
                .manifest
                .protected_files
                .iter()
                .any(|path| path.eq_ignore_ascii_case(file_path))
        {
            return Err(format!("folder disk file {file_path:?} is protected"));
        }
        Ok(())
    }
}

pub fn import_legacy_block_disk(source: &Path, destination: &Path) -> Result<FolderDisk, String> {
    let disk_id = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| "legacy disk filename must contain a valid disk ID".to_owned())?;
    validate_disk_id(disk_id)?;
    if destination.file_name().and_then(|name| name.to_str()) != Some(disk_id) {
        return Err("folder disk destination name must match the legacy disk ID".to_owned());
    }
    match fs::symlink_metadata(destination) {
        Ok(_) => return Err("folder disk destination already exists".to_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("cannot inspect folder disk destination: {error}")),
    }
    let raw = fs::read_to_string(source)
        .map_err(|error| format!("cannot read legacy block disk: {error}"))?;
    let (blocks, read_only) = parse_disk(&raw, disk_id)?;
    let capacity_bytes = (blocks.len() as u64)
        .checked_mul(BLOCK_SIZE as u64)
        .ok_or_else(|| "legacy disk capacity overflow".to_owned())?
        .max(1);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create folder disk destination parent: {error}"))?;
    }
    fs::create_dir(destination)
        .map_err(|error| format!("cannot create folder disk destination: {error}"))?;
    let result = (|| {
        let mut manifest = Map::new();
        manifest.insert(
            "version".to_owned(),
            Value::from(FOLDER_DISK_MANIFEST_VERSION),
        );
        manifest.insert("id".to_owned(), Value::String(disk_id.to_owned()));
        manifest.insert("label".to_owned(), Value::String(disk_id.to_owned()));
        manifest.insert("capacity_bytes".to_owned(), Value::from(capacity_bytes));
        manifest.insert("read_only".to_owned(), Value::Bool(read_only));
        let manifest_bytes = serde_json::to_vec_pretty(&Value::Object(manifest))
            .map_err(|error| format!("cannot serialize folder disk manifest: {error}"))?;
        fs::write(destination.join("disk.json"), manifest_bytes)
            .map_err(|error| format!("cannot write folder disk manifest: {error}"))?;
        let block_directory = destination.join("legacy-blocks");
        fs::create_dir(&block_directory)
            .map_err(|error| format!("cannot create legacy block export folder: {error}"))?;
        for (number, block) in blocks {
            let path = block_directory.join(format!("{number:05}.bin"));
            fs::write(path, block)
                .map_err(|error| format!("cannot export legacy disk block: {error}"))?;
        }
        FolderDisk::open(destination)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    result
}

fn collect_folder_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<(String, u64)>,
) -> Result<(), String> {
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("cannot list folder disk files: {error}"))?
    {
        let entry = entry.map_err(|error| format!("cannot list folder disk files: {error}"))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| format!("cannot inspect folder disk entry: {error}"))?;
        if file_type.is_symlink() {
            return Err(format!("folder disk contains unsupported symlink {path:?}"));
        }
        if file_type.is_dir() {
            collect_folder_files(root, &path, files)?;
        } else if file_type.is_file() {
            if path == root.join("disk.json") {
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .map_err(|_| "folder disk entry escapes mounted root".to_owned())?;
            let name = relative.to_string_lossy().replace('\\', "/");
            validate_file_path(&name)?;
            let length = entry
                .metadata()
                .map_err(|error| format!("cannot inspect folder disk file: {error}"))?
                .len();
            files.push((name, length));
        } else {
            return Err(format!("folder disk contains unsupported entry {path:?}"));
        }
    }
    Ok(())
}

pub struct DiskDevice {
    storage_dir: PathBuf,
    file_root: Option<PathBuf>,
    current_disk: Option<String>,
    blocks: BTreeMap<u16, [u8; BLOCK_SIZE]>,
    write_protected: bool,
}

impl DiskDevice {
    pub fn new(storage_dir: PathBuf) -> Self {
        Self {
            storage_dir,
            file_root: None,
            current_disk: None,
            blocks: BTreeMap::new(),
            write_protected: false,
        }
    }

    pub fn set_file_root(&mut self, root: PathBuf) -> Result<(), String> {
        let root = if root.exists() {
            fs::canonicalize(&root)
                .map_err(|error| format!("cannot configure module disk folder: {error}"))?
        } else {
            let parent = root.parent().unwrap_or_else(|| Path::new("."));
            let name = root
                .file_name()
                .ok_or_else(|| "cannot configure module disk folder".to_owned())?;
            fs::canonicalize(parent)
                .map_err(|error| format!("cannot configure module disk folder: {error}"))?
                .join(name)
        };
        self.file_root = Some(root);
        Ok(())
    }

    pub fn current_disk(&self) -> Option<&str> {
        self.current_disk.as_deref()
    }

    pub fn list_disks(&self) -> Result<Vec<String>, String> {
        fs::create_dir_all(&self.storage_dir)
            .map_err(|error| format!("cannot create disk storage folder: {error}"))?;
        let entries = fs::read_dir(&self.storage_dir)
            .map_err(|error| format!("cannot list virtual disks: {error}"))?;
        let mut disks = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| format!("cannot list virtual disks: {error}"))?;
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
                && let Some(id) = path.file_stem().and_then(|stem| stem.to_str())
                && valid_disk_id(id)
            {
                disks.push(id.to_owned());
            }
        }
        disks.sort();
        Ok(disks)
    }

    pub fn is_protected(&self, disk_id: &str) -> Result<bool, String> {
        validate_disk_id(disk_id)?;
        if self.current_disk.as_deref() == Some(disk_id) {
            return Ok(self.write_protected);
        }
        let path = self.storage_dir.join(format!("{disk_id}.json"));
        let raw = match fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(format!("cannot inspect disk {disk_id}: {error}")),
        };
        let value: Value = serde_json::from_str(&raw)
            .map_err(|error| format!("cannot inspect disk {disk_id}: {error}"))?;
        let object = value
            .as_object()
            .ok_or_else(|| format!("disk {disk_id} has an invalid image format"))?;
        Ok(object.get("protected").and_then(Value::as_bool) == Some(true))
    }

    pub fn load_disk(&mut self, disk_id: &str) -> Result<(), String> {
        validate_disk_id(disk_id)?;
        fs::create_dir_all(&self.storage_dir)
            .map_err(|error| format!("cannot create disk storage folder: {error}"))?;
        let path = self.storage_dir.join(format!("{disk_id}.json"));
        let existed = path.exists();
        let (blocks, protected) = if existed {
            let raw = fs::read_to_string(&path)
                .map_err(|error| format!("cannot read disk {disk_id}: {error}"))?;
            parse_disk(&raw, disk_id)?
        } else {
            (BTreeMap::new(), false)
        };
        if self
            .current_disk
            .as_deref()
            .is_some_and(|current| current != disk_id)
        {
            self.save()?;
        }
        let previous = (
            self.current_disk.clone(),
            self.blocks.clone(),
            self.write_protected,
        );
        self.current_disk = Some(disk_id.to_owned());
        self.blocks = blocks;
        self.write_protected = protected;
        if !existed && let Err(error) = self.save() {
            self.current_disk = previous.0;
            self.blocks = previous.1;
            self.write_protected = previous.2;
            return Err(error);
        }
        Ok(())
    }

    pub fn unload_disk(&mut self) -> Result<(), String> {
        if self.current_disk.is_some() {
            self.save()?;
            self.current_disk = None;
            self.blocks.clear();
            self.write_protected = false;
        }
        Ok(())
    }

    pub fn set_protected(&mut self, protected: bool) -> Result<(), String> {
        if self.current_disk.is_none() {
            return Err("no virtual disk is mounted".to_owned());
        }
        let previous = self.write_protected;
        self.write_protected = protected;
        if let Err(error) = self.save() {
            self.write_protected = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn read_block(&self, pointer: u16) -> Result<[u8; BLOCK_SIZE], String> {
        if self.current_disk.is_none() {
            return Err("no virtual disk is mounted".to_owned());
        }
        Ok(self
            .blocks
            .get(&pointer)
            .copied()
            .unwrap_or([b' '; BLOCK_SIZE]))
    }

    pub fn write_block(&mut self, pointer: u16, data: &[u8]) -> Result<(), String> {
        if data.len() != BLOCK_SIZE {
            return Err("disk blocks must contain exactly 256 bytes".to_owned());
        }
        if self.current_disk.is_none() {
            return Err("no virtual disk is mounted".to_owned());
        }
        if self.write_protected {
            return Err(format!(
                "disk {} is write-protected",
                self.current_disk.as_deref().unwrap_or_default()
            ));
        }
        let mut block = [0; BLOCK_SIZE];
        block.copy_from_slice(data);
        let previous = self.blocks.insert(pointer, block);
        if let Err(error) = self.save() {
            if let Some(previous) = previous {
                self.blocks.insert(pointer, previous);
            } else {
                self.blocks.remove(&pointer);
            }
            return Err(error);
        }
        Ok(())
    }

    pub fn list_files(&self) -> Result<Vec<String>, String> {
        let root = self
            .file_root
            .as_ref()
            .ok_or_else(|| "the running module has no disk folder".to_owned())?;
        let mut names = Vec::new();
        if root.exists() {
            list_files_under(root, root, &mut names)?;
        }
        names.sort();
        Ok(names)
    }

    pub fn read_file(
        &self,
        file_path: &str,
        offset: u32,
        length: usize,
    ) -> Result<(Vec<u8>, u32), String> {
        if length > MAX_FILE_READ_SIZE {
            return Err("invalid virtual file read range".to_owned());
        }
        let relative_path = validate_file_path(file_path)?;
        let root = self
            .file_root
            .as_ref()
            .ok_or_else(|| "the running module has no disk folder".to_owned())?;
        let path = root.join(relative_path);
        let resolved = path
            .canonicalize()
            .map_err(|error| format!("cannot read virtual file {file_path:?}: {error}"))?;
        if !resolved.starts_with(root) {
            return Err("virtual file path escapes the disk".to_owned());
        }
        let mut file = File::open(&resolved)
            .map_err(|error| format!("cannot read virtual file {file_path:?}: {error}"))?;
        let file_size = file
            .metadata()
            .map_err(|error| format!("cannot inspect virtual file {file_path:?}: {error}"))?
            .len();
        let file_size = u32::try_from(file_size)
            .map_err(|_| format!("virtual file {file_path:?} exceeds the 32-bit size limit"))?;
        file.seek(SeekFrom::Start(u64::from(offset)))
            .map_err(|error| format!("cannot seek virtual file {file_path:?}: {error}"))?;
        let mut data = vec![0; length];
        let count = file
            .read(&mut data)
            .map_err(|error| format!("cannot read virtual file {file_path:?}: {error}"))?;
        data.truncate(count);
        Ok((data, file_size))
    }

    fn save(&self) -> Result<(), String> {
        let disk_id = self
            .current_disk
            .as_deref()
            .ok_or_else(|| "no virtual disk is mounted".to_owned())?;
        validate_disk_id(disk_id)?;
        let mut object = Map::new();
        if self.write_protected {
            object.insert("protected".to_owned(), Value::Bool(true));
        }
        for (block_number, block) in &self.blocks {
            let text = block
                .iter()
                .map(|byte| char::from(*byte))
                .collect::<String>();
            object.insert(block_number.to_string(), Value::String(text));
        }
        let contents = serde_json::to_vec_pretty(&Value::Object(object))
            .map_err(|error| format!("cannot serialize disk {disk_id}: {error}"))?;
        fs::create_dir_all(&self.storage_dir)
            .map_err(|error| format!("cannot create disk storage folder: {error}"))?;
        let path = self.storage_dir.join(format!("{disk_id}.json"));
        let temporary = self.storage_dir.join(format!("{disk_id}.json.tmp"));
        let mut file = File::create(&temporary)
            .map_err(|error| format!("cannot save disk {disk_id}: {error}"))?;
        file.write_all(&contents)
            .and_then(|()| file.write_all(b"\n"))
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("cannot save disk {disk_id}: {error}"))?;
        fs::rename(&temporary, &path)
            .map_err(|error| format!("cannot save disk {disk_id}: {error}"))?;
        Ok(())
    }
}

fn parse_disk(raw: &str, disk_id: &str) -> Result<(BTreeMap<u16, [u8; BLOCK_SIZE]>, bool), String> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|error| format!("cannot read disk {disk_id}: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| format!("disk {disk_id} has an invalid image format"))?;
    let protected = object.get("protected").and_then(Value::as_bool) == Some(true);
    let mut blocks = BTreeMap::new();
    for (key, value) in object {
        if key == "protected" {
            continue;
        }
        let block_number = key
            .parse::<u16>()
            .map_err(|_| format!("disk {disk_id} has invalid block data: invalid block entry"))?;
        let text = value
            .as_str()
            .ok_or_else(|| format!("disk {disk_id} has invalid block data: invalid block entry"))?;
        let mut block = [0; BLOCK_SIZE];
        let mut count = 0;
        for character in text.chars() {
            if count >= BLOCK_SIZE || character as u32 > 255 {
                return Err(format!(
                    "disk {disk_id} has invalid block data: disk blocks must contain exactly 256 bytes"
                ));
            }
            block[count] = character as u8;
            count += 1;
        }
        if count != BLOCK_SIZE {
            return Err(format!(
                "disk {disk_id} has invalid block data: disk blocks must contain exactly 256 bytes"
            ));
        }
        blocks.insert(block_number, block);
    }
    Ok((blocks, protected))
}

fn valid_disk_id(disk_id: &str) -> bool {
    disk_id.len() == 8
        && disk_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn validate_disk_id(disk_id: &str) -> Result<(), String> {
    if valid_disk_id(disk_id) {
        Ok(())
    } else {
        Err("disk ID must be exactly 8 ASCII letters, digits, '_' or '-'".to_owned())
    }
}

fn validate_file_path(file_path: &str) -> Result<PathBuf, String> {
    if file_path.is_empty()
        || file_path.len() > 128
        || !file_path.is_ascii()
        || file_path.contains('\\')
        || file_path.starts_with('/')
    {
        return Err("invalid virtual file path".to_owned());
    }
    let mut path = PathBuf::new();
    for part in file_path.split('/') {
        if part.is_empty()
            || part == "."
            || part == ".."
            || !part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte))
        {
            return Err("invalid virtual file path".to_owned());
        }
        path.push(part);
    }
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("invalid virtual file path".to_owned());
    }
    Ok(path)
}

fn list_files_under(root: &Path, directory: &Path, output: &mut Vec<String>) -> Result<(), String> {
    let entries =
        fs::read_dir(directory).map_err(|error| format!("cannot list files on disk: {error}"))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("cannot list files on disk: {error}"))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| format!("cannot inspect module disk file: {error}"))?;
        if file_type.is_dir() {
            list_files_under(root, &path, output)?;
        } else if file_type.is_file()
            || (file_type.is_symlink()
                && path.is_file()
                && path
                    .canonicalize()
                    .is_ok_and(|resolved| resolved.starts_with(root)))
        {
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            let name = relative.to_string_lossy().replace('\\', "/");
            if validate_file_path(&name).is_ok() {
                output.push(name);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("sc16-{label}-{unique}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn blocks_persist_with_write_protection_and_reload() {
        let root = temp_dir("disk");
        let mut disk = DiskDevice::new(root.clone());
        disk.load_disk("TEST0001").unwrap();
        assert_eq!(disk.read_block(7).unwrap(), [b' '; BLOCK_SIZE]);
        let data = std::array::from_fn(|index| index as u8);
        disk.write_block(7, &data).unwrap();
        disk.set_protected(true).unwrap();
        assert!(disk.write_block(8, &data).is_err());
        disk.unload_disk().unwrap();
        disk.load_disk("TEST0001").unwrap();
        assert_eq!(disk.read_block(7).unwrap(), data);
        assert!(disk.is_protected("TEST0001").unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_disk_images_are_rejected_without_replacing_the_mounted_disk() {
        let root = temp_dir("disk-invalid");
        fs::write(root.join("BADJSON1.json"), b"[]").unwrap();
        fs::write(root.join("BADBLOCK.json"), br#"{"1":"short"}"#).unwrap();
        fs::write(
            root.join("BADRANGE.json"),
            format!(r#"{{"65536":"{}"}}"#, "x".repeat(BLOCK_SIZE)),
        )
        .unwrap();

        let mut disk = DiskDevice::new(root.clone());
        disk.load_disk("GOOD0001").unwrap();
        for disk_id in ["BADJSON1", "BADBLOCK", "BADRANGE", "../evil"] {
            assert!(disk.load_disk(disk_id).is_err(), "{disk_id}");
            assert_eq!(disk.current_disk(), Some("GOOD0001"));
        }
        assert!(disk.read_block(0).is_ok());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn imports_legacy_block_disk_into_non_overwriting_folder_disk() {
        let root = temp_dir("disk-import");
        let source = root.join("LEGACY01.json");
        let destination = root.join("LEGACY01");
        fs::write(
            &source,
            format!(
                r#"{{"protected":true,"3":"{}","9":"{}"}}"#,
                "A".repeat(BLOCK_SIZE),
                "B".repeat(BLOCK_SIZE)
            ),
        )
        .unwrap();

        let disk = import_legacy_block_disk(&source, &destination).unwrap();

        assert_eq!(disk.manifest().id, "LEGACY01");
        assert!(disk.manifest().read_only);
        assert_eq!(disk.manifest().capacity_bytes, (2 * BLOCK_SIZE) as u64);
        assert_eq!(disk.used_bytes().unwrap(), (2 * BLOCK_SIZE) as u64);
        assert_eq!(
            disk.read_file("legacy-blocks/00003.bin", 0, BLOCK_SIZE)
                .unwrap()
                .0,
            vec![b'A'; BLOCK_SIZE]
        );
        assert_eq!(
            disk.read_file("legacy-blocks/00009.bin", 0, BLOCK_SIZE)
                .unwrap()
                .0,
            vec![b'B'; BLOCK_SIZE]
        );
        assert!(import_legacy_block_disk(&source, &destination).is_err());
        assert!(source.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn disk_listing_is_sorted_and_ignores_invalid_names() {
        let root = temp_dir("disk-list");
        fs::write(root.join("zeta0001.json"), b"{}").unwrap();
        fs::write(root.join("ALPHA001.json"), b"{}").unwrap();
        fs::write(root.join("invalid.json"), b"{}").unwrap();
        fs::write(root.join("TOOLONG99.json"), b"{}").unwrap();
        let disk = DiskDevice::new(root.clone());
        assert_eq!(disk.list_disks().unwrap(), ["ALPHA001", "zeta0001"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn virtual_files_are_listed_and_read_with_bounds_and_path_checks() {
        let root = temp_dir("files");
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("nested").join("hello.txt"), b"world!").unwrap();
        let mut disk = DiskDevice::new(root.join("store"));
        disk.set_file_root(root.clone()).unwrap();
        assert_eq!(disk.list_files().unwrap(), ["nested/hello.txt"]);
        assert_eq!(
            disk.read_file("nested/hello.txt", 1, 3).unwrap(),
            (b"orl".to_vec(), 6)
        );
        assert_eq!(
            disk.read_file("nested/hello.txt", 99, 16).unwrap(),
            (Vec::new(), 6)
        );
        for invalid_path in [
            "",
            "../secret",
            "nested/../hello.txt",
            "nested//hello.txt",
            "nested\\hello.txt",
            "/nested/hello.txt",
            "nested/hello world.txt",
        ] {
            assert!(
                disk.read_file(invalid_path, 0, 1).is_err(),
                "{invalid_path:?}"
            );
        }
        assert!(
            disk.read_file("nested/hello.txt", 0, MAX_FILE_READ_SIZE + 1)
                .is_err()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn file_listing_skips_names_outside_the_virtual_file_path_format() {
        let root = temp_dir("files-invalid");
        fs::write(root.join("valid.txt"), b"valid").unwrap();
        fs::write(root.join("not valid.txt"), b"invalid").unwrap();
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("nested").join("also_valid.json"), b"valid").unwrap();
        let mut disk = DiskDevice::new(root.join("store"));
        disk.set_file_root(root.clone()).unwrap();
        assert_eq!(
            disk.list_files().unwrap(),
            ["nested/also_valid.json", "valid.txt"]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parses_versioned_folder_disk_manifest() {
        let manifest = parse_folder_disk_manifest(
            r#"{"version":1,"id":"DISK0001","label":"System","capacity_bytes":65536,"read_only":true,"protected_files":["kernel.bin","assets/font.dat"]}"#,
        )
        .unwrap();

        assert_eq!(manifest.version, 1);
        assert_eq!(manifest.id, "DISK0001");
        assert_eq!(manifest.label, "System");
        assert_eq!(manifest.capacity_bytes, 65_536);
        assert!(manifest.read_only);
        assert_eq!(manifest.protected_files, ["kernel.bin", "assets/font.dat"]);

        let empty = parse_folder_disk_manifest(
            r#"{"version":1,"id":"DISK0002","label":"Data","capacity_bytes":1,"read_only":false}"#,
        )
        .unwrap();
        assert!(empty.protected_files.is_empty());
    }

    #[test]
    fn rejects_invalid_or_boot_selecting_folder_disk_manifests() {
        for manifest in [
            r#"{"version":2,"id":"DISK0001","label":"System","capacity_bytes":65536,"read_only":false}"#,
            r#"{"version":1,"id":"../evil","label":"System","capacity_bytes":65536,"read_only":false}"#,
            r#"{"version":1,"id":"DISK0001","label":"System","capacity_bytes":0,"read_only":false}"#,
            r#"{"version":1,"id":"DISK0001","label":"System","capacity_bytes":65536,"read_only":false,"protected_files":["../kernel.bin"]}"#,
            r#"{"version":1,"id":"DISK0001","label":"System","capacity_bytes":65536,"read_only":false,"protected_files":["kernel.bin","kernel.bin"]}"#,
            r#"{"version":1,"id":"DISK0001","label":"System","capacity_bytes":65536,"read_only":false,"executable":"kernel.bin"}"#,
        ] {
            assert!(parse_folder_disk_manifest(manifest).is_err(), "{manifest}");
        }
    }

    #[test]
    fn mounts_folder_disk_only_when_manifest_matches_directory() {
        let root = temp_dir("folder-mount");
        let disk_dir = root.join("DISK0001");
        fs::create_dir_all(&disk_dir).unwrap();
        fs::write(
            disk_dir.join("disk.json"),
            r#"{"version":1,"id":"DISK0001","label":"System","capacity_bytes":65536,"read_only":true}"#,
        )
        .unwrap();

        let disk = FolderDisk::open(&disk_dir).unwrap();
        assert_eq!(disk.manifest().id, "DISK0001");
        assert_eq!(disk.root().file_name().unwrap(), "DISK0001");

        let mismatch_dir = root.join("DISK0002");
        fs::create_dir_all(&mismatch_dir).unwrap();
        fs::write(
            mismatch_dir.join("disk.json"),
            r#"{"version":1,"id":"DISK0001","label":"System","capacity_bytes":65536,"read_only":true}"#,
        )
        .unwrap();
        assert!(FolderDisk::open(&mismatch_dir).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_folder_disk_whose_existing_files_exceed_capacity() {
        let root = temp_dir("folder-over-capacity");
        let disk_root = root.join("DISK0001");
        fs::create_dir_all(&disk_root).unwrap();
        fs::write(
            disk_root.join("disk.json"),
            r#"{"version":1,"id":"DISK0001","label":"Data","capacity_bytes":3,"read_only":false}"#,
        )
        .unwrap();
        fs::write(disk_root.join("data.bin"), b"four").unwrap();

        assert!(FolderDisk::open(&disk_root).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discovers_folder_disks_in_id_order_and_ignores_other_entries() {
        let root = temp_dir("folder-discovery");
        for id in ["DISK0002", "DISK0001"] {
            let disk_dir = root.join(id);
            fs::create_dir_all(&disk_dir).unwrap();
            fs::write(
                disk_dir.join("disk.json"),
                format!(
                    r#"{{"version":1,"id":"{id}","label":"Test","capacity_bytes":65536,"read_only":false}}"#
                ),
            )
            .unwrap();
        }
        fs::create_dir_all(root.join("not-a-disk")).unwrap();
        fs::write(root.join("notes.txt"), b"not a disk").unwrap();

        let disks = FolderDisk::discover(&root).unwrap();

        assert_eq!(
            disks
                .iter()
                .map(|disk| disk.manifest().id.as_str())
                .collect::<Vec<_>>(),
            ["DISK0001", "DISK0002"]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discovery_fails_closed_on_invalid_candidate_disk() {
        let root = temp_dir("folder-discovery-invalid");
        fs::create_dir(root.join("DISK0001")).unwrap();

        assert!(FolderDisk::discover(&root).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn folder_disk_accounts_regular_file_bytes_and_enforces_write_modes() {
        let root = temp_dir("folder-capacity");
        let disk_root = root.join("DISK0001");
        fs::create_dir_all(&disk_root).unwrap();
        fs::write(
            disk_root.join("disk.json"),
            r#"{"version":1,"id":"DISK0001","label":"Data","capacity_bytes":5,"read_only":false,"protected_files":["locked.dat"]}"#,
        )
        .unwrap();
        fs::write(disk_root.join("data.dat"), b"abc").unwrap();
        fs::write(disk_root.join("locked.dat"), b"x").unwrap();

        let disk = FolderDisk::open(&disk_root).unwrap();
        assert_eq!(disk.used_bytes().unwrap(), 4);
        assert_eq!(
            disk.read_file("data.dat", 0, 16).unwrap(),
            (b"abc".to_vec(), 3)
        );
        disk.write_file("data.dat", b"abcd").unwrap();
        assert_eq!(disk.used_bytes().unwrap(), 5);
        assert!(disk.write_file("data.dat", b"oversize").is_err());
        assert!(disk.copy_file("locked.dat", "copy.dat").is_err());
        assert!(disk.write_file("locked.dat", b"changed").is_err());
        assert_eq!(fs::read(disk_root.join("locked.dat")).unwrap(), b"x");
        assert!(disk.write_file("disk.json", b"{}").is_err());

        let read_only_root = root.join("DISK0002");
        fs::create_dir(&read_only_root).unwrap();
        fs::write(
            read_only_root.join("disk.json"),
            r#"{"version":1,"id":"DISK0002","label":"Archive","capacity_bytes":5,"read_only":true}"#,
        )
        .unwrap();
        fs::write(read_only_root.join("existing.txt"), b"keep").unwrap();
        let read_only_disk = FolderDisk::open(&read_only_root).unwrap();
        assert!(read_only_disk.write_file("new.dat", b"x").is_err());
        assert!(
            read_only_disk
                .copy_file("existing.txt", "copy.txt")
                .is_err()
        );
        assert!(read_only_disk.create_dir("new-dir").is_err());
        assert!(read_only_disk.delete_file("existing.txt").is_err());
        assert!(
            read_only_disk
                .rename_file("existing.txt", "renamed.txt")
                .is_err()
        );
        assert_eq!(
            fs::read(read_only_root.join("existing.txt")).unwrap(),
            b"keep"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn folder_disk_file_operations_are_confined_and_protect_files() {
        let root = temp_dir("folder-operations");
        let disk_root = root.join("DISK0001");
        fs::create_dir_all(&disk_root).unwrap();
        fs::write(
            disk_root.join("disk.json"),
            r#"{"version":1,"id":"DISK0001","label":"Data","capacity_bytes":1024,"read_only":false,"protected_files":["locked.txt"]}"#,
        )
        .unwrap();
        fs::write(disk_root.join("locked.txt"), b"keep").unwrap();

        let disk = FolderDisk::open(&disk_root).unwrap();
        disk.create_dir("docs").unwrap();
        disk.write_file("docs/a.txt", b"hello").unwrap();
        disk.copy_file("locked.txt", "docs/copied.txt").unwrap();
        assert_eq!(
            disk.list_files().unwrap(),
            ["docs/a.txt", "docs/copied.txt", "locked.txt"]
        );
        assert_eq!(disk.read_file("docs/copied.txt", 0, 16).unwrap().0, b"keep");
        assert!(disk.copy_file("locked.txt", "locked.txt").is_err());
        assert!(disk.copy_file("docs/a.txt", "locked.txt").is_err());
        assert_eq!(disk.read_file("docs/a.txt", 1, 3).unwrap().0, b"ell");
        disk.rename_file("docs/a.txt", "docs/b.txt").unwrap();
        assert_eq!(
            disk.list_files().unwrap(),
            ["docs/b.txt", "docs/copied.txt", "locked.txt"]
        );
        disk.delete_file("docs/b.txt").unwrap();
        assert_eq!(
            disk.list_files().unwrap(),
            ["docs/copied.txt", "locked.txt"]
        );

        assert!(disk.create_dir("../escape").is_err());
        assert!(disk.write_file("../escape.txt", b"no").is_err());
        assert!(disk.delete_file("locked.txt").is_err());
        assert!(disk.rename_file("locked.txt", "renamed.txt").is_err());
        assert_eq!(fs::read(disk_root.join("locked.txt")).unwrap(), b"keep");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn folder_disk_directory_operations_preserve_protected_subtrees() {
        let root = temp_dir("folder-directories");
        let disk_root = root.join("DISK0001");
        fs::create_dir_all(disk_root.join("secure")).unwrap();
        fs::write(
            disk_root.join("disk.json"),
            r#"{"version":1,"id":"DISK0001","label":"Data","capacity_bytes":1024,"read_only":false,"protected_files":["secure/private.txt"]}"#,
        )
        .unwrap();
        fs::write(disk_root.join("secure/private.txt"), b"keep").unwrap();
        let disk = FolderDisk::open(&disk_root).unwrap();

        let root_entries = disk.list_dir(None).unwrap();
        assert_eq!(
            root_entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["secure"]
        );
        assert!(root_entries[0].is_directory);
        let secure_entries = disk.list_dir(Some("secure")).unwrap();
        assert_eq!(
            secure_entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["private.txt"]
        );
        assert!(!secure_entries[0].is_directory);
        assert!(disk.rename_entry("secure", "moved").is_err());
        assert!(disk.delete_entry("secure").is_err());
        assert!(disk.delete_entry("secure/private.txt").is_err());

        disk.create_dir("empty").unwrap();
        disk.rename_entry("empty", "renamed").unwrap();
        assert_eq!(
            disk.list_dir(None)
                .unwrap()
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["renamed", "secure"]
        );
        disk.delete_entry("renamed").unwrap();
        assert_eq!(
            disk.list_dir(None)
                .unwrap()
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["secure"]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn folder_disk_boot_selection_uses_fixed_executable_names() {
        fn make_disk(root: &Path, id: &str, executables: &[&str]) -> FolderDisk {
            let disk_root = root.join(id);
            fs::create_dir_all(&disk_root).unwrap();
            fs::write(
                disk_root.join("disk.json"),
                format!(
                    r#"{{"version":1,"id":"{id}","label":"Boot","capacity_bytes":65536,"read_only":true}}"#
                ),
            )
            .unwrap();
            let image = crate::image::ProgramImage {
                entry_point: 0,
                segments: vec![crate::image::ImageSegment::new(0, vec![0, 0])],
            };
            for executable in executables {
                crate::image::write_image(&disk_root.join(executable), &image).unwrap();
            }
            FolderDisk::open(&disk_root).unwrap()
        }

        let root = temp_dir("folder-boot-selection");
        let user_disk = make_disk(&root, "USER0001", &["main.bin"]);
        let user_image = user_disk.load_boot_image().unwrap();
        assert_eq!(user_image.mode, FolderBootMode::User);
        assert_eq!(user_image.image.entry_point, 0);

        let kernel_disk = make_disk(&root, "KERN0001", &["kernel.bin"]);
        let kernel_image = kernel_disk.load_boot_image().unwrap();
        assert_eq!(kernel_image.mode, FolderBootMode::Kernel);
        assert_eq!(kernel_image.image.entry_point, 0);

        let both_disk = make_disk(&root, "BOTH0001", &["main.bin", "kernel.bin"]);
        assert!(
            both_disk
                .load_boot_image()
                .unwrap_err()
                .contains("NOT_BOOT")
        );
        let neither_disk = make_disk(&root, "NONE0001", &[]);
        assert!(
            neither_disk
                .load_boot_image()
                .unwrap_err()
                .contains("NOT_BOOT")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
