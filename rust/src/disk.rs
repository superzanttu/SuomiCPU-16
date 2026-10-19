use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};

const BLOCK_SIZE: usize = 256;
const MAX_FILE_READ_SIZE: usize = 4096;

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
        assert!(disk.read_file("../secret", 0, 1).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
