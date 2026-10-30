//! Builds and installs programs from validated repository manifests.
//! Provides catalog discovery, staged builds, ownership/conflict checks,
//! recoverable disk installation, and legacy program-disk migration.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use serde_json::{Value, json};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const SUPPORTED_GUEST_PROGRAMS: &[&str] = &["sisuos"];

fn retry<T>(mut operation: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    for attempt in 0..5 {
        match operation() {
            Err(error)
                if cfg!(windows)
                    && matches!(error.raw_os_error(), Some(5 | 32 | 33))
                    && attempt < 4 =>
            {
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            result => return result,
        }
    }
    unreachable!()
}

#[derive(Debug)]
pub struct Executable {
    pub source: PathBuf,
    pub output: String,
    pub embedded_c: Option<PathBuf>,
    pub base: Option<u32>,
}

#[derive(Debug)]
pub struct Program {
    pub slug: String,
    pub name: String,
    pub legacy_disk: String,
    pub capacity: u64,
    pub assets: Option<PathBuf>,
    pub executables: Vec<Executable>,
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .ok_or_else(|| format!("program manifest requires string {key}").into())
}

fn fields(value: &Value, allowed: &[&str]) -> Result<()> {
    let object = value
        .as_object()
        .ok_or("program manifest entries must be objects")?;
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("unknown program manifest field {key:?}").into());
        }
    }
    Ok(())
}

fn relative(value: &str) -> Result<PathBuf> {
    if value.is_empty()
        || value.contains('\\')
        || value.split('/').any(|part| {
            part.is_empty()
                || part == "."
                || part == ".."
                || !part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        })
        || Path::new(value)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(format!("invalid program-relative path {value:?}").into());
    }
    Ok(PathBuf::from(value))
}

fn regular(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("expected a regular file: {}", path.display()).into());
    }
    Ok(())
}

fn directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!("expected a real directory: {}", path.display()).into());
    }
    Ok(())
}

fn local_path(root: &Path, value: &str) -> Result<PathBuf> {
    let path = root.join(relative(value)?);
    let mut parent = path.parent();
    while let Some(dir) = parent {
        directory(dir)?;
        if dir == root {
            break;
        }
        parent = dir.parent();
    }
    Ok(path)
}

fn source(root: &Path, value: &str) -> Result<PathBuf> {
    let path = local_path(root, value)?;
    regular(&path)?;
    Ok(path)
}

pub fn catalog(repository: &Path) -> Result<Vec<Program>> {
    let root = repository.join("programs");
    directory(&root)?;
    let mut programs = Vec::new();
    let mut ids = HashSet::new();
    let mut legacy_ids = HashSet::new();
    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        let path = entry.path();
        directory(&path)?;
        let manifest = path.join("program.json");
        regular(&manifest)?;
        let value: Value = serde_json::from_slice(&fs::read(&manifest)?)?;
        fields(
            &value,
            &[
                "version",
                "slug",
                "name",
                "legacy_disk",
                "capacity_bytes",
                "boot",
                "assets",
                "executables",
            ],
        )?;
        if value["version"] != 1 {
            return Err(format!("unsupported manifest version: {}", manifest.display()).into());
        }
        let slug = text(&value, "slug")?;
        crate::disk::validate_disk_id(slug)?;
        if slug != entry.file_name().to_string_lossy()
            || !slug
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            || !slug.as_bytes()[0].is_ascii_alphanumeric()
            || slug.ends_with('-')
            || !ids.insert(slug.to_owned())
        {
            return Err(format!("invalid or duplicate program slug {slug:?}").into());
        }
        let name = text(&value, "name")?;
        if name.trim().is_empty() || name.chars().any(char::is_control) {
            return Err("invalid program display name".into());
        }
        let legacy_disk = text(&value, "legacy_disk")?;
        crate::disk::validate_disk_id(legacy_disk)?;
        if !legacy_ids.insert(legacy_disk.to_ascii_lowercase()) {
            return Err("duplicate legacy disk ID".into());
        }
        let capacity = value["capacity_bytes"]
            .as_u64()
            .filter(|n| *n > 0)
            .ok_or("program manifest requires positive capacity_bytes")?;
        let assets = match &value["assets"] {
            Value::Null => None,
            Value::String(value) => {
                let path = local_path(&path, value)?;
                directory(&path)?;
                Some(path)
            }
            _ => return Err("program assets must be a directory path or null".into()),
        };
        let extras = value["executables"]
            .as_array()
            .ok_or("executables must be an array")?;
        let mut executables = Vec::new();
        let mut outputs = HashSet::new();
        for (index, executable) in std::iter::once(&value["boot"])
            .chain(extras.iter())
            .enumerate()
        {
            fields(executable, &["source", "output", "embedded_c", "base"])?;
            let output = text(executable, "output")?;
            relative(output)?;
            if output.contains('/')
                || !output.ends_with(".bin")
                || !outputs.insert(output.to_ascii_lowercase())
                || (index == 0 && !matches!(output, "main.bin" | "kernel.bin"))
            {
                return Err(format!("invalid executable output {output:?}").into());
            }
            let embedded_c = executable
                .get("embedded_c")
                .map(|v| source(&path, v.as_str().ok_or("embedded_c must be a source path")?))
                .transpose()?;
            if embedded_c.is_some() && (index != 0 || output != "kernel.bin") {
                return Err("embedded C requires a kernel boot executable".into());
            }
            let base = executable
                .get("base")
                .map(|v| {
                    v.as_u64()
                        .and_then(|n| u32::try_from(n).ok())
                        .ok_or("invalid executable base")
                })
                .transpose()?;
            if embedded_c.is_some() && base.is_some() {
                return Err("embedded C and base cannot be combined".into());
            }
            executables.push(Executable {
                source: source(&path, text(executable, "source")?)?,
                output: output.to_owned(),
                embedded_c,
                base,
            });
        }
        programs.push(Program {
            slug: slug.to_owned(),
            name: name.to_owned(),
            legacy_disk: legacy_disk.to_owned(),
            capacity,
            assets,
            executables,
        });
    }
    programs.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(programs)
}

fn files(root: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    fn visit(root: &Path, path: &Path, result: &mut BTreeMap<String, Vec<u8>>) -> Result<()> {
        directory(path)?;
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                visit(root, &path, result)?;
            } else {
                regular(&path)?;
                let name = path
                    .strip_prefix(root)?
                    .to_str()
                    .ok_or("non-UTF8 asset path")?
                    .replace('\\', "/");
                relative(&name)?;
                if result.keys().any(|key| key.eq_ignore_ascii_case(&name)) {
                    return Err(format!("case-insensitive asset collision: {name}").into());
                }
                result.insert(
                    name,
                    retry(|| fs::read(&path))
                        .map_err(|error| format!("cannot read {}: {error}", path.display()))?,
                );
            }
        }
        Ok(())
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result)?;
    Ok(result)
}

fn write_files(root: &Path, entries: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    fs::create_dir_all(root)?;
    for (name, data) in entries {
        let path = root.join(relative(name)?);
        fs::create_dir_all(path.parent().ok_or("missing output parent")?)?;
        retry(|| fs::write(&path, data))
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    }
    Ok(())
}

fn preserve_directories(source: &Path, destination: &Path) -> Result<()> {
    directory(source)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let target = destination.join(entry.file_name());
            fs::create_dir_all(&target)?;
            preserve_directories(&entry.path(), &target)?;
        }
    }
    Ok(())
}

fn fingerprint(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    format!("{}:{hash:016x}", bytes.len())
}

fn inventory(entries: &BTreeMap<String, Vec<u8>>) -> Value {
    Value::Object(
        entries
            .iter()
            .filter(|(name, _)| name.as_str() != "disk.json")
            .map(|(name, data)| (name.clone(), Value::String(fingerprint(data))))
            .collect(),
    )
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    let temporary = path.with_file_name(format!(
        ".sc16-{}.pending",
        path.file_name()
            .ok_or("missing JSON filename")?
            .to_string_lossy()
    ));
    if temporary.exists() {
        regular(&temporary)?;
        retry(|| fs::remove_file(&temporary))?;
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(&serde_json::to_vec_pretty(value)?)?;
    file.sync_all()?;
    drop(file);
    retry(|| fs::rename(&temporary, path))?;
    Ok(())
}

pub fn build(repository: &Path, program: &Program) -> Result<PathBuf> {
    let root = repository.join("build").join("programs");
    fs::create_dir_all(&root)?;
    let destination = root.join(&program.slug);
    let mut entries = match &program.assets {
        Some(path) => files(path)?,
        None => BTreeMap::new(),
    };
    if entries.contains_key("disk.json") || entries.contains_key(".sc16-owned.json") {
        return Err("seed assets cannot supply package metadata".into());
    }
    for executable in &program.executables {
        if entries
            .keys()
            .any(|key| key.eq_ignore_ascii_case(&executable.output))
        {
            return Err(format!("seed asset conflicts with {}", executable.output).into());
        }
        let image = if let Some(embedded) = &executable.embedded_c {
            crate::image::build_kernel_with_embedded_c_source(&executable.source, embedded)?
        } else if let Some(base) = executable.base {
            crate::image::build_c_program_at(&executable.source, base)?
        } else {
            crate::image::build_source(&executable.source)?
        };
        entries.insert(
            executable.output.clone(),
            crate::image::encode_image(&image)?,
        );
    }
    let owned = inventory(&entries);
    entries.insert(
        ".sc16-owned.json".to_owned(),
        serde_json::to_vec_pretty(&owned)?,
    );
    entries.insert("disk.json".to_owned(), serde_json::to_vec_pretty(&json!({
        "version":1,"id":program.slug,"label":program.name,"capacity_bytes":program.capacity,
        "read_only":false,"protected_files":program.executables.iter().map(|e| &e.output).collect::<Vec<_>>(),
    }))?);
    let staging_root = root.join(format!(".stage-{}", program.slug));
    if staging_root.exists() {
        return Err(format!(
            "unfinished build at {}; inspect and remove it before rebuilding",
            staging_root.display()
        )
        .into());
    }
    let staging = staging_root.join(&program.slug);
    write_files(&staging, &entries)?;
    crate::disk::FolderDisk::open(&staging)?;
    if destination.exists() {
        directory(&destination)?;
        retry(|| fs::remove_dir_all(&destination))?;
    }
    retry(|| fs::rename(&staging, &destination))?;
    retry(|| fs::remove_dir(&staging_root))?;
    println!("Built {} -> {}", program.slug, destination.display());
    Ok(destination)
}

fn recover_install(disks: &Path, slug: &str, announce: bool) -> Result<()> {
    let journal = disks.join(format!(".install-{slug}.json"));
    if !journal.exists() {
        return Ok(());
    }
    let live = disks.join(slug);
    let backup_root = disks.join(format!(".backup-{slug}"));
    let backup = backup_root.join(slug);
    if !live.exists() && backup.exists() {
        retry(|| fs::rename(&backup, &live))?;
    }
    if live.exists() {
        crate::disk::FolderDisk::open(&live)?;
    }
    if backup.exists() {
        directory(&backup)?;
        retry(|| fs::remove_dir_all(&backup))?;
    }
    if backup_root.exists() {
        retry(|| fs::remove_dir(&backup_root))?;
    }
    let staging_root = disks.join(format!(".install-{slug}"));
    if staging_root.exists() {
        directory(&staging_root)?;
        retry(|| fs::remove_dir_all(&staging_root))?;
    }
    retry(|| fs::remove_file(&journal))?;
    if announce {
        println!("Recovered interrupted installation of {slug}");
    }
    Ok(())
}

pub fn install(repository: &Path, program: &Program) -> Result<()> {
    let disks = repository.join("disks");
    fs::create_dir_all(&disks)?;
    directory(&disks)?;
    recover_install(&disks, &program.slug, true)?;
    let built = build(repository, program)?;
    let live = disks.join(&program.slug);
    let mut package = files(&built)?;
    let package_inventory: Value = serde_json::from_slice(&package[".sc16-owned.json"])?;
    if disks.join(&program.legacy_disk).exists() {
        return Err("obsolete program disk exists; run migrate-program-disks --apply first".into());
    }
    if live.exists() {
        let mounted = crate::disk::FolderDisk::open(&live)?;
        if mounted.manifest().read_only {
            return Err(format!("{} is read-only; installation refused", program.slug).into());
        }
        let mut existing = files(&live)?;
        let owned: Value = match existing.get(".sc16-owned.json") {
            Some(data) => serde_json::from_slice(data)?,
            None => json!({}),
        };
        let old_owned = owned
            .as_object()
            .ok_or("invalid installed ownership inventory")?;
        for (name, expected) in old_owned {
            relative(name)?;
            if package_inventory.get(name).is_none() {
                if let Some(data) = existing.get(name) {
                    if expected.as_str() != Some(fingerprint(data).as_str()) {
                        return Err(format!("modified retired package file: {name}").into());
                    }
                    existing.remove(name);
                }
            }
        }
        for (name, data) in &package {
            if matches!(name.as_str(), "disk.json" | ".sc16-owned.json") {
                continue;
            }
            if existing
                .keys()
                .any(|key| key != name && key.eq_ignore_ascii_case(name))
            {
                return Err(format!("case-insensitive local file conflict: {name}").into());
            }
            if let Some(previous) = existing.get(name) {
                let executable = program.executables.iter().any(|e| e.output == *name);
                let unchanged = old_owned.get(name).and_then(Value::as_str)
                    == Some(fingerprint(previous).as_str());
                let adopting = old_owned.get(name).is_none() && (executable || previous == data);
                if !unchanged && !adopting {
                    return Err(format!(
                        "local file conflicts with package output: {name}; nothing installed"
                    )
                    .into());
                }
            }
        }
        let mut manifest: Value = serde_json::from_slice(&existing["disk.json"])?;
        let protected = manifest["protected_files"]
            .as_array_mut()
            .ok_or("invalid protected_files")?;
        for executable in &program.executables {
            if !protected
                .iter()
                .any(|v| v.as_str() == Some(&executable.output))
            {
                protected.push(Value::String(executable.output.clone()));
            }
        }
        package.insert(
            "disk.json".to_owned(),
            serde_json::to_vec_pretty(&manifest)?,
        );
        existing.extend(package);
        package = existing;
    }
    let staging_root = disks.join(format!(".install-{}", program.slug));
    let staging = staging_root.join(&program.slug);
    if staging_root.exists() {
        return Err(format!("unexpected install staging at {}", staging_root.display()).into());
    }
    let journal = disks.join(format!(".install-{}.json", program.slug));
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&journal)?;
    let transaction: Result<()> = (|| {
        write_files(&staging, &package)?;
        if live.exists() {
            preserve_directories(&live, &staging)?;
        }
        crate::disk::FolderDisk::open(&staging)?;
        if live.exists() {
            let backup_root = disks.join(format!(".backup-{}", program.slug));
            fs::create_dir(&backup_root)?;
            retry(|| fs::rename(&live, backup_root.join(&program.slug)))?;
        }
        retry(|| fs::rename(&staging, &live))?;
        Ok(())
    })();
    if let Err(recovery) = recover_install(&disks, &program.slug, transaction.is_err()) {
        return Err(match transaction {
            Ok(()) => {
                format!("install committed but cleanup failed: {recovery}; retry installation")
            }
            Err(error) => format!(
                "installation failed: {error}; recovery failed: {recovery}; retry installation"
            ),
        }
        .into());
    }
    transaction?;
    println!("Installed {} -> {}", program.slug, live.display());
    Ok(())
}

pub fn migrate(repository: &Path, apply: bool) -> Result<()> {
    let programs = catalog(repository)?;
    let disks = repository.join("disks");
    let disk_paths = crate::disk::FolderDisk::discover_paths(&disks)?;
    let journal = disks.join(".program-migration.json");
    let pending = journal.exists();
    let mut operations = Vec::new();
    for program in &programs {
        let old = disks.join(&program.legacy_disk);
        let new = disks.join(&program.slug);
        if old.exists()
            && disk_paths.iter().any(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&program.slug)
            })
        {
            return Err(format!(
                "migration conflict: {} and {} both exist; no merge attempted",
                old.display(),
                new.display()
            )
            .into());
        }
        if old.exists() {
            crate::disk::FolderDisk::open(&old)?;
            operations.push(json!({"old":program.legacy_disk,"new":program.slug}));
            println!("{} -> {}", program.legacy_disk, program.slug);
        }
    }
    let state: Value = if pending {
        regular(&journal)?;
        serde_json::from_slice(&fs::read(&journal)?)?
    } else {
        json!({"operations":operations})
    };
    let operations = state["operations"]
        .as_array()
        .ok_or("invalid migration journal")?;
    for operation in operations {
        let old = text(operation, "old")?;
        let new = text(operation, "new")?;
        if !programs
            .iter()
            .any(|p| p.legacy_disk == old && p.slug == new)
        {
            return Err("migration journal contains an unknown disk mapping".into());
        }
        if !disks.join(old).exists() && !disks.join(new).exists() {
            return Err(format!("both migration paths missing for {old} -> {new}").into());
        }
    }
    if !apply {
        println!(
            "Dry run: {} disk rename(s); use --apply to migrate or resume.",
            operations.len()
        );
        return Ok(());
    }
    let settings = repository.join("config").join("bios.json");
    let mut settings_value: Option<Value> = if settings.exists() {
        regular(&settings)?;
        let value: Value = serde_json::from_slice(&fs::read(&settings)?)?;
        if !value.is_object() {
            return Err("BIOS settings must be an object".into());
        }
        Some(value)
    } else {
        None
    };
    if !pending {
        write_json(&journal, &state)?;
    }
    for operation in operations {
        let old = text(operation, "old")?;
        let new = text(operation, "new")?;
        let target = disks.join(new);
        if disks.join(old).exists() {
            retry(|| fs::rename(disks.join(old), &target))?;
        }
        let manifest_path = target.join("disk.json");
        let mut manifest: Value = serde_json::from_slice(&fs::read(&manifest_path)?)?;
        if manifest["id"] != old && manifest["id"] != new {
            return Err(format!("unexpected manifest ID during migration of {old}").into());
        }
        manifest["id"] = Value::String(new.to_owned());
        write_json(&manifest_path, &manifest)?;
        crate::disk::FolderDisk::open(&target)?;
    }
    if let Some(value) = &mut settings_value {
        if let Some(program) = programs
            .iter()
            .find(|p| value["preferred_disk"].as_str() == Some(&p.legacy_disk))
        {
            value["preferred_disk"] = Value::String(program.slug.clone());
            write_json(&settings, value)?;
        }
    }
    retry(|| fs::remove_file(&journal))?;
    println!("Program disk migration complete; writable contents and BIOS settings preserved.");
    Ok(())
}

pub fn command(repository: &Path, command: &str, arguments: &[String]) -> Result<()> {
    if command == "migrate-program-disks" {
        if arguments.len() != 1 || !matches!(arguments[0].as_str(), "--dry-run" | "--apply") {
            return Err("usage: sc16build migrate-program-disks --dry-run|--apply".into());
        }
        return migrate(repository, arguments[0] == "--apply");
    }
    let all = matches!(command, "build-programs" | "install-programs");
    if (all && !arguments.is_empty()) || (!all && arguments.len() != 1) {
        return Err(format!(
            "usage: sc16build {command}{}",
            if all { "" } else { " <slug>" }
        )
        .into());
    }
    let programs = catalog(repository)?;
    let selected: Vec<_> = programs
        .iter()
        .filter(|p| {
            SUPPORTED_GUEST_PROGRAMS.contains(&p.slug.as_str()) && (all || p.slug == arguments[0])
        })
        .collect();
    if selected.is_empty() {
        if !all && programs.iter().any(|program| program.slug == arguments[0]) {
            return Err(format!(
                "program {:?} is archived and unsupported by the current hardware; only SisuOS is supported",
                arguments[0]
            )
            .into());
        }
        return Err(format!(
            "unknown program {:?}; use a canonical programs/<slug> name",
            arguments[0]
        )
        .into());
    }
    for program in selected {
        if command.starts_with("install") {
            install(repository, program).map_err(|error| format!("{}: {error}", program.slug))?;
        } else {
            build(repository, program).map_err(|error| format!("{}: {error}", program.slug))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "sc16-programs-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let source = root.join("programs/hello/src");
            fs::create_dir_all(&source).unwrap();
            fs::create_dir_all(root.join("programs/hello/assets/disk")).unwrap();
            fs::write(source.join("main.c"), "int main(void) { return 42; }").unwrap();
            fs::write(source.join("extra.c"), "not the entry point").unwrap();
            fs::write(root.join("programs/hello/assets/disk/seed.txt"), b"seed").unwrap();
            write_json(
                &root.join("programs/hello/program.json"),
                &json!({
                    "version":1,"slug":"hello","name":"Hello","legacy_disk":"APP00001",
                    "capacity_bytes":65536,"boot":{"source":"src/main.c","output":"main.bin"},
                    "assets":"assets/disk","executables":[],
                }),
            )
            .unwrap();
            fs::create_dir_all(root.join("examples")).unwrap();
            fs::write(
                root.join("examples/example.c"),
                "not an installable program",
            )
            .unwrap();
            fs::create_dir_all(root.join("disks")).unwrap();
            Self(root)
        }

        fn program(&self) -> Program {
            catalog(&self.0).unwrap().remove(0)
        }

        fn legacy(&self) -> PathBuf {
            let built = build(&self.0, &self.program()).unwrap();
            let old = self.0.join("disks/APP00001");
            write_files(&old, &files(&built).unwrap()).unwrap();
            let manifest = old.join("disk.json");
            let mut value: Value = serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
            value["id"] = json!("APP00001");
            write_json(&manifest, &value).unwrap();
            old
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            retry(|| fs::remove_dir_all(&self.0)).unwrap();
        }
    }

    #[test]
    fn catalog_uses_manifest_entry_and_excludes_examples() {
        let fixture = Fixture::new();
        let catalog = catalog(&fixture.0).unwrap();
        assert_eq!(catalog.len(), 1);
        assert!(catalog[0].executables[0].source.ends_with("src/main.c"));
        let built = build(&fixture.0, &catalog[0]).unwrap();
        let disk = crate::disk::FolderDisk::open(&built).unwrap();
        let image = disk.load_boot_image().unwrap().image;
        let mut cpu = crate::Cpu::with_memory_size(1 << 20).unwrap();
        cpu.load_program(&image.segments, image.entry_point)
            .unwrap();
        cpu.run(10_000).unwrap();
        assert!(!cpu.running);
        assert_eq!(cpu.registers[0], 42);
    }

    #[test]
    fn catalog_rejects_escaping_paths_mismatched_names_and_duplicate_outputs() {
        let fixture = Fixture::new();
        let path = fixture.0.join("programs/hello/program.json");
        let original: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        for (field, invalid) in [
            ("slug", json!("Hello")),
            ("version", json!(2)),
            ("capacity_bytes", json!(0)),
        ] {
            let mut value = original.clone();
            value[field] = invalid;
            write_json(&path, &value).unwrap();
            assert!(catalog(&fixture.0).is_err());
        }
        for invalid in [
            "../outside.c",
            "src/../../outside.c",
            "src\\main.c",
            "/main.c",
        ] {
            let mut value = original.clone();
            value["boot"]["source"] = json!(invalid);
            write_json(&path, &value).unwrap();
            assert!(catalog(&fixture.0).is_err());
        }
        let mut value = original;
        value["executables"] = json!([{"source":"src/main.c","output":"MAIN.bin"}]);
        write_json(&path, &value).unwrap();
        assert!(catalog(&fixture.0).is_err());
    }

    #[test]
    fn installs_preserve_data_settings_and_update_owned_files() {
        let fixture = Fixture::new();
        let program = fixture.program();
        install(&fixture.0, &program).unwrap();
        let live = fixture.0.join("disks/hello");
        fs::write(live.join("save.dat"), b"saved game").unwrap();
        fs::create_dir_all(live.join("empty/saved-directory")).unwrap();
        let manifest = live.join("disk.json");
        let mut value: Value = serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
        value["capacity_bytes"] = json!(131072);
        value["label"] = json!("My label");
        value["protected_files"]
            .as_array_mut()
            .unwrap()
            .push(json!("save.dat"));
        write_json(&manifest, &value).unwrap();
        fs::write(
            fixture.0.join("programs/hello/assets/disk/seed.txt"),
            b"new seed",
        )
        .unwrap();
        install(&fixture.0, &program).unwrap();
        assert_eq!(fs::read(live.join("seed.txt")).unwrap(), b"new seed");
        assert_eq!(fs::read(live.join("save.dat")).unwrap(), b"saved game");
        assert!(live.join("empty/saved-directory").is_dir());
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(manifest).unwrap()).unwrap(),
            value
        );
    }

    #[test]
    fn modified_seeds_executables_and_case_collisions_are_not_overwritten() {
        let fixture = Fixture::new();
        let program = fixture.program();
        install(&fixture.0, &program).unwrap();
        let live = fixture.0.join("disks/hello");
        let main = fs::read(live.join("main.bin")).unwrap();
        fs::write(live.join("seed.txt"), b"edited").unwrap();
        assert!(
            install(&fixture.0, &program)
                .unwrap_err()
                .to_string()
                .contains("conflicts")
        );
        assert_eq!(fs::read(live.join("seed.txt")).unwrap(), b"edited");
        fs::write(live.join("seed.txt"), b"seed").unwrap();
        fs::write(live.join("main.bin"), b"edited executable").unwrap();
        assert!(install(&fixture.0, &program).is_err());
        assert_eq!(
            fs::read(live.join("main.bin")).unwrap(),
            b"edited executable"
        );
        fs::write(live.join("main.bin"), main).unwrap();
        fs::rename(live.join("seed.txt"), live.join("Seed.txt")).unwrap();
        assert!(
            install(&fixture.0, &program)
                .unwrap_err()
                .to_string()
                .contains("case-insensitive")
        );
        assert_eq!(fs::read(live.join("Seed.txt")).unwrap(), b"seed");
    }

    #[test]
    fn retired_owned_seeds_are_removed_without_touching_local_files() {
        let fixture = Fixture::new();
        let program = fixture.program();
        install(&fixture.0, &program).unwrap();
        let live = fixture.0.join("disks/hello");
        fs::write(live.join("local.txt"), b"keep").unwrap();
        fs::remove_file(fixture.0.join("programs/hello/assets/disk/seed.txt")).unwrap();
        install(&fixture.0, &program).unwrap();
        assert!(!live.join("seed.txt").exists());
        assert_eq!(fs::read(live.join("local.txt")).unwrap(), b"keep");
    }

    #[test]
    fn first_install_adopts_only_matching_seeds_and_known_executables() {
        let fixture = Fixture::new();
        let old = fixture.legacy();
        migrate(&fixture.0, true).unwrap();
        assert!(!old.exists());
        let live = fixture.0.join("disks/hello");
        fs::remove_file(live.join(".sc16-owned.json")).unwrap();
        fs::write(live.join("main.bin"), b"old compiler output").unwrap();
        fs::write(live.join("seed.txt"), b"locally modified seed").unwrap();
        assert!(install(&fixture.0, &fixture.program()).is_err());
        fs::write(live.join("seed.txt"), b"seed").unwrap();
        install(&fixture.0, &fixture.program()).unwrap();
        crate::disk::FolderDisk::open(&live)
            .unwrap()
            .load_boot_image()
            .unwrap();
    }

    #[test]
    fn read_only_and_capacity_failures_leave_installed_data_unchanged() {
        let fixture = Fixture::new();
        let program = fixture.program();
        install(&fixture.0, &program).unwrap();
        let live = fixture.0.join("disks/hello");
        let manifest = live.join("disk.json");
        let mut value: Value = serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
        value["read_only"] = json!(true);
        write_json(&manifest, &value).unwrap();
        let before = files(&live).unwrap();
        assert!(install(&fixture.0, &program).is_err());
        assert_eq!(files(&live).unwrap(), before);
        value["read_only"] = json!(false);
        value["capacity_bytes"] = json!(
            crate::disk::FolderDisk::open(&live)
                .unwrap()
                .used_bytes()
                .unwrap()
        );
        write_json(&manifest, &value).unwrap();
        let before = files(&live).unwrap();
        fs::write(
            fixture.0.join("programs/hello/assets/disk/seed.txt"),
            vec![b'x'; 32768],
        )
        .unwrap();
        assert!(install(&fixture.0, &program).is_err());
        assert_eq!(files(&live).unwrap(), before);
    }

    #[test]
    fn install_recovery_restores_backup_and_discards_incomplete_staging() {
        let fixture = Fixture::new();
        install(&fixture.0, &fixture.program()).unwrap();
        let disks = fixture.0.join("disks");
        let before = files(&disks.join("hello")).unwrap();
        fs::create_dir(disks.join(".backup-hello")).unwrap();
        fs::rename(disks.join("hello"), disks.join(".backup-hello/hello")).unwrap();
        fs::create_dir(disks.join(".install-hello")).unwrap();
        fs::write(disks.join(".install-hello/incomplete"), b"partial").unwrap();
        fs::write(disks.join(".install-hello.json"), b"").unwrap();
        recover_install(&disks, "hello", true).unwrap();
        assert_eq!(files(&disks.join("hello")).unwrap(), before);
        assert!(!disks.join(".install-hello").exists());
        assert!(!disks.join(".backup-hello").exists());
        fs::create_dir(disks.join(".backup-hello")).unwrap();
        fs::rename(disks.join("hello"), disks.join(".backup-hello/hello")).unwrap();
        fs::write(disks.join(".install-hello.json"), b"").unwrap();
        let program = fixture.program();
        fs::write(&program.executables[0].source, "broken C").unwrap();
        assert!(install(&fixture.0, &program).is_err());
        assert_eq!(files(&disks.join("hello")).unwrap(), before);
    }

    #[test]
    fn migration_preserves_bytes_and_settings_and_dry_run_changes_nothing() {
        let fixture = Fixture::new();
        let old = fixture.legacy();
        fs::write(old.join("save.dat"), b"keep").unwrap();
        let before = files(&old).unwrap();
        let settings = json!({"preferred_disk":"APP00001","network_name":"SC16-EXAMPLE","settings":{"test":42},"startup_behavior":"selector","version":1});
        fs::create_dir_all(fixture.0.join("config")).unwrap();
        write_json(&fixture.0.join("config").join("bios.json"), &settings).unwrap();
        migrate(&fixture.0, false).unwrap();
        assert_eq!(files(&old).unwrap(), before);
        assert!(!fixture.0.join("disks/.program-migration.json").exists());
        migrate(&fixture.0, true).unwrap();
        let live = fixture.0.join("disks/hello");
        let mut after = files(&live).unwrap();
        after.insert("disk.json".to_owned(), before["disk.json"].clone());
        assert_eq!(after, before);
        let mut expected = settings;
        expected["preferred_disk"] = json!("hello");
        assert_eq!(
            serde_json::from_slice::<Value>(
                &fs::read(fixture.0.join("config").join("bios.json")).unwrap()
            )
            .unwrap(),
            expected
        );
        migrate(&fixture.0, true).unwrap();
        assert!(!old.exists());
    }

    #[test]
    fn migration_conflicts_are_preflighted_and_interrupted_renames_resume() {
        let fixture = Fixture::new();
        let old = fixture.legacy();
        let disks = fixture.0.join("disks");
        fs::create_dir(disks.join("hello")).unwrap();
        assert!(migrate(&fixture.0, true).is_err());
        assert!(old.exists());
        assert!(!disks.join(".program-migration.json").exists());
        fs::remove_dir(disks.join("hello")).unwrap();
        write_json(
            &disks.join(".program-migration.json"),
            &json!({"operations":[{"old":"APP00001","new":"hello"}]}),
        )
        .unwrap();
        fs::rename(old, disks.join("hello")).unwrap();
        migrate(&fixture.0, true).unwrap();
        assert_eq!(
            crate::disk::FolderDisk::open(&disks.join("hello"))
                .unwrap()
                .manifest()
                .id,
            "hello"
        );
        assert!(!disks.join(".program-migration.json").exists());
    }

    #[test]
    fn disk_ids_accept_readable_names_and_reject_windows_and_path_hazards() {
        for valid in [
            "a",
            "sisuos",
            "a-readable-program-name",
            "DISK0001",
            &"x".repeat(32),
        ] {
            crate::disk::validate_disk_id(valid).unwrap();
        }
        for invalid in [
            "",
            "../escape",
            "a/b",
            "a\\b",
            "CON",
            "nul",
            "LPT1",
            "COM9",
            &"x".repeat(33),
        ] {
            assert!(crate::disk::validate_disk_id(invalid).is_err(), "{invalid}");
        }
        let mut cpu = crate::Cpu::with_memory_size(1 << 20).unwrap();
        assert!(
            cpu.mount_data_disk("APP00001")
                .unwrap_err()
                .contains("does not exist")
        );
        assert!(
            cpu.mount_data_disk("SisuOS-1")
                .unwrap_err()
                .contains("does not exist")
        );
    }
}
