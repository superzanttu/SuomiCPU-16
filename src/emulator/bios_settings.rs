use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::hash::{BuildHasher, Hash, Hasher, RandomState};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_ADDITIONAL_SETTINGS: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BiosSettingValue {
    Boolean(bool),
    Integer(i64),
    String(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StartupBehavior {
    #[default]
    Selector,
    BootPreferred,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BiosSettings {
    pub preferred_disk: Option<String>,
    pub startup_behavior: StartupBehavior,
    pub network_name: String,
    pub additional: BTreeMap<String, BiosSettingValue>,
}

impl Default for BiosSettings {
    fn default() -> Self {
        Self {
            preferred_disk: None,
            startup_behavior: StartupBehavior::Selector,
            network_name: random_network_name(),
            additional: BTreeMap::new(),
        }
    }
}

fn random_network_name() -> String {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let mut hasher = RandomState::new().build_hasher();
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    SEQUENCE.fetch_add(1, Ordering::Relaxed).hash(&mut hasher);
    format!("SC16-{:011X}", hasher.finish() & 0x0000_0fff_ffff_ffff)
}

impl BiosSettings {
    pub fn load(path: &Path) -> Result<Self, String> {
        let contents = match fs::read(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => {
                return Err(format!(
                    "cannot read BIOS settings {}: {error}",
                    path.display()
                ));
            }
        };
        let document: serde_json::Value = serde_json::from_slice(&contents)
            .map_err(|error| format!("invalid BIOS settings {}: {error}", path.display()))?;
        Self::from_json(document)
            .map_err(|error| format!("invalid BIOS settings {}: {error}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        let document = self.to_json();
        let bytes = serde_json::to_vec_pretty(&document)
            .map_err(|error| format!("cannot encode BIOS settings: {error}"))?;
        save_atomically(path, &bytes)
            .map_err(|error| format!("cannot save BIOS settings {}: {error}", path.display()))
    }

    pub fn validate(&self) -> Result<(), String> {
        if let Some(disk) = &self.preferred_disk {
            crate::disk::validate_disk_id(disk)?;
        }
        if self.network_name.is_empty()
            || self.network_name.len() > 16
            || !self
                .network_name
                .bytes()
                .all(|byte| (0x20..=0x7e).contains(&byte))
        {
            return Err("SC-NET node name must contain 1-16 printable ASCII bytes".to_owned());
        }
        if self.additional.len() > MAX_ADDITIONAL_SETTINGS {
            return Err(format!(
                "at most {MAX_ADDITIONAL_SETTINGS} additional BIOS settings are supported"
            ));
        }
        for (name, value) in &self.additional {
            if name.is_empty()
                || name.len() > 16
                || !name.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
                || matches!(
                    name.as_str(),
                    "version" | "preferred_disk" | "startup_behavior" | "network_name" | "settings"
                )
            {
                return Err(format!(
                    "additional BIOS setting names must be 1-16 printable ASCII bytes: {name:?}"
                ));
            }
            if let BiosSettingValue::String(value) = value {
                if value.len() > 16 || !value.bytes().all(|byte| (0x20..=0x7e).contains(&byte)) {
                    return Err(format!(
                        "additional BIOS setting {name:?} must be at most 16 printable ASCII bytes"
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn set_network_name(&mut self, network_name: String) -> Result<(), String> {
        let previous = std::mem::replace(&mut self.network_name, network_name);
        if let Err(error) = self.validate() {
            self.network_name = previous;
            return Err(error);
        }
        Ok(())
    }

    fn from_json(document: serde_json::Value) -> Result<Self, String> {
        let object = document
            .as_object()
            .ok_or_else(|| "top-level value must be an object".to_owned())?;
        if let Some(version) = object.get("version") {
            if version.as_u64() != Some(1) {
                return Err("unsupported BIOS settings version".to_owned());
            }
        }
        let preferred_disk = match object.get("preferred_disk") {
            None | Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::String(value)) => Some(value.clone()),
            Some(_) => return Err("preferred_disk must be a string or null".to_owned()),
        };
        let startup_behavior = match object
            .get("startup_behavior")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("selector")
        {
            "selector" => StartupBehavior::Selector,
            "boot_preferred" => StartupBehavior::BootPreferred,
            _ => return Err("startup_behavior must be selector or boot_preferred".to_owned()),
        };
        let network_name = match object.get("network_name") {
            None => random_network_name(),
            Some(serde_json::Value::String(value)) => value.clone(),
            Some(_) => return Err("network_name must be a string".to_owned()),
        };
        let mut additional = BTreeMap::new();
        if let Some(values) = object.get("settings") {
            let values = values
                .as_object()
                .ok_or_else(|| "settings must be an object".to_owned())?;
            for (name, value) in values {
                let typed_value = if let Some(integer) = value.as_i64() {
                    BiosSettingValue::Integer(integer)
                } else if let Some(boolean) = value.as_bool() {
                    BiosSettingValue::Boolean(boolean)
                } else if let Some(string) = value.as_str() {
                    BiosSettingValue::String(string.to_owned())
                } else {
                    return Err(format!(
                        "additional setting {name:?} must be a boolean, integer, or string"
                    ));
                };
                additional.insert(name.clone(), typed_value);
            }
        }
        let settings = Self {
            preferred_disk,
            startup_behavior,
            network_name,
            additional,
        };
        settings.validate()?;
        Ok(settings)
    }

    fn to_json(&self) -> serde_json::Value {
        let mut additional = serde_json::Map::new();
        for (name, value) in &self.additional {
            let value = match value {
                BiosSettingValue::Boolean(boolean) => serde_json::Value::from(*boolean),
                BiosSettingValue::Integer(integer) => serde_json::Value::from(*integer),
                BiosSettingValue::String(string) => serde_json::Value::from(string.clone()),
            };
            additional.insert(name.clone(), value);
        }
        serde_json::json!({
            "version": 1,
            "preferred_disk": self.preferred_disk,
            "startup_behavior": match self.startup_behavior {
                StartupBehavior::Selector => "selector",
                StartupBehavior::BootPreferred => "boot_preferred",
            },
            "network_name": self.network_name,
            "settings": additional,
        })
    }
}

fn save_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let file_name = path.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "settings path has no file name",
        )
    })?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(
        "{}.{}.{}.tmp",
        file_name.to_string_lossy(),
        std::process::id(),
        nonce
    ));
    let backup = parent.join(format!(
        "{}.{}.{}.bak",
        file_name.to_string_lossy(),
        std::process::id(),
        nonce
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    drop(file);

    let had_existing = path.exists();
    if had_existing {
        if let Err(error) = fs::rename(path, &backup) {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
    }
    if let Err(error) = fs::rename(&temporary, path) {
        if had_existing {
            let _ = fs::rename(&backup, path);
        }
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    if had_existing {
        fs::remove_file(backup)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temporary_settings_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "suomicpu-bios-settings-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[test]
    fn missing_settings_generate_a_full_length_sc_net_node_name() {
        let path = temporary_settings_path();
        let mut settings = BiosSettings::load(&path).unwrap();
        assert_eq!(settings.network_name.len(), 16);
        assert!(settings.network_name.starts_with("SC16-"));
        assert!(
            settings.network_name[5..]
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        );
        settings.preferred_disk = Some("anaclock".to_owned());
        settings.startup_behavior = StartupBehavior::BootPreferred;
        settings.network_name = "ROBOT".to_owned();
        settings
            .additional
            .insert("integer_option".to_owned(), BiosSettingValue::Integer(12));
        settings
            .additional
            .insert("boolean_option".to_owned(), BiosSettingValue::Boolean(true));
        settings.additional.insert(
            "string_option".to_owned(),
            BiosSettingValue::String("value".to_owned()),
        );

        settings.save(&path).unwrap();

        assert_eq!(BiosSettings::load(&path).unwrap(), settings);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn invalid_network_names_and_setting_values_are_rejected() {
        let mut settings = BiosSettings::default();
        assert!(
            settings
                .set_network_name("0123456789ABCDEFG".to_owned())
                .is_err()
        );
        assert!(settings.set_network_name(String::new()).is_err());
        assert!(
            settings
                .set_network_name("SC16-0123456789A".to_owned())
                .is_ok()
        );
        let parsed = BiosSettings::from_json(serde_json::json!({
            "settings": {
                "enabled": true,
                "attempts": 42,
                "label": "example"
            }
        }))
        .unwrap();
        assert_eq!(
            parsed.additional.get("enabled"),
            Some(&BiosSettingValue::Boolean(true))
        );
        assert_eq!(
            parsed.additional.get("attempts"),
            Some(&BiosSettingValue::Integer(42))
        );
        assert_eq!(
            parsed.additional.get("label"),
            Some(&BiosSettingValue::String("example".to_owned()))
        );
        assert!(
            BiosSettings::from_json(serde_json::json!({
                "settings": { "label": "a string that is longer than sixteen" }
            }))
            .is_err()
        );
        assert!(
            BiosSettings::from_json(serde_json::json!({
                "settings": { "enabled": null }
            }))
            .is_err()
        );
        assert!(
            BiosSettings::from_json(serde_json::json!({
                "settings": {
                    "one": true,
                    "two": true,
                    "three": true,
                    "four": true,
                    "five": true,
                    "six": true,
                    "seven": true,
                    "eight": true,
                    "nine": true
                }
            }))
            .is_err()
        );
    }
}
