use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const ACCOUNT_STORE_VERSION: u32 = 1;
pub const ACCOUNT_STORE_PATH: &str = ".sisuos/accounts.json";
pub const MAX_ACCOUNT_STORE_BYTES: usize = 16 * 1024;
const MAX_USERS: usize = 32;
const MAX_GROUPS: usize = 16;
const MAX_FILE_PERMISSIONS: usize = 64;
const MAX_PASSWORD_BYTES: usize = 128;
const MAX_USERNAME_BYTES: usize = 32;
const MAX_ARGON2_MEMORY_KIB: u32 = 19 * 1024;
const MAX_ARGON2_ITERATIONS: u32 = 2;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UserAccount {
    pub name: String,
    pub uid: u32,
    pub primary_gid: u32,
    pub supplementary_gids: Vec<u32>,
    pub home: String,
    pub password_hash: Option<String>,
    pub disabled: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GroupAccount {
    pub name: String,
    pub gid: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FilePermission {
    pub path: String,
    pub uid: u32,
    pub gid: u32,
    pub mode: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AccountStore {
    pub version: u32,
    pub users: Vec<UserAccount>,
    pub groups: Vec<GroupAccount>,
    pub files: Vec<FilePermission>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AccountError {
    InvalidMetadata(String),
    InvalidName,
    InvalidHome,
    InvalidPassword,
    PermissionDenied,
    AlreadyInitialized,
    NameInUse,
    UnknownUser,
    UnknownGroup,
    FilePermissionAlreadyExists,
    CapacityExceeded,
    HashingFailed(String),
    Storage(String),
}

impl AccountStore {
    pub fn first_boot() -> Self {
        Self {
            version: ACCOUNT_STORE_VERSION,
            users: vec![UserAccount {
                name: "root".to_owned(),
                uid: 0,
                primary_gid: 0,
                supplementary_gids: Vec::new(),
                home: "root".to_owned(),
                password_hash: None,
                disabled: false,
            }],
            groups: vec![GroupAccount {
                name: "root".to_owned(),
                gid: 0,
            }],
            files: Vec::new(),
        }
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, AccountError> {
        if bytes.is_empty() || bytes.len() > MAX_ACCOUNT_STORE_BYTES {
            return Err(AccountError::InvalidMetadata(
                "account metadata size is invalid".to_owned(),
            ));
        }
        let store: Self = serde_json::from_slice(bytes)
            .map_err(|error| AccountError::InvalidMetadata(error.to_string()))?;
        store.validate()?;
        Ok(store)
    }

    pub fn load(disk: &crate::disk::FolderDisk) -> Result<Self, AccountError> {
        match disk
            .read_kernel_account_store()
            .map_err(AccountError::Storage)?
        {
            Some(bytes) => Self::decode(&bytes),
            None => Ok(Self::first_boot()),
        }
    }

    pub fn save(&self, disk: &crate::disk::FolderDisk) -> Result<(), AccountError> {
        let bytes = self.encode()?;
        disk.write_kernel_account_store(&bytes)
            .map_err(AccountError::Storage)
    }

    pub fn encode(&self) -> Result<Vec<u8>, AccountError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)
            .map_err(|error| AccountError::InvalidMetadata(error.to_string()))?;
        if bytes.len() > MAX_ACCOUNT_STORE_BYTES {
            return Err(AccountError::CapacityExceeded);
        }
        Ok(bytes)
    }

    pub fn users(&self) -> &[UserAccount] {
        &self.users
    }

    pub fn groups(&self) -> &[GroupAccount] {
        &self.groups
    }

    pub fn file_permissions(&self) -> &[FilePermission] {
        &self.files
    }

    pub fn user(&self, name: &str) -> Option<&UserAccount> {
        self.users.iter().find(|user| user.name == name)
    }

    pub fn group(&self, name: &str) -> Option<&GroupAccount> {
        self.groups.iter().find(|group| group.name == name)
    }

    pub fn initialize_root_password(&mut self, password: &str) -> Result<(), AccountError> {
        let root = self
            .users
            .iter_mut()
            .find(|user| user.uid == 0 && user.name == "root")
            .ok_or_else(|| AccountError::InvalidMetadata("root account is missing".to_owned()))?;
        if root.password_hash.is_some() {
            return Err(AccountError::AlreadyInitialized);
        }
        root.password_hash = Some(hash_password(password)?);
        self.validate()
    }

    pub fn authenticate(&self, name: &str, password: &str) -> Option<&UserAccount> {
        let user = self.users.iter().find(|user| user.name == name)?;
        if user.disabled || password.len() > MAX_PASSWORD_BYTES {
            return None;
        }
        let encoded = user.password_hash.as_ref()?;
        let parsed = PasswordHash::new(encoded).ok()?;
        argon2()
            .verify_password(password.as_bytes(), &parsed)
            .ok()?;
        Some(user)
    }

    pub fn add_group(&mut self, name: &str) -> Result<u32, AccountError> {
        validate_name(name)?;
        if self.groups.iter().any(|group| group.name == name) {
            return Err(AccountError::NameInUse);
        }
        if self.groups.len() >= MAX_GROUPS {
            return Err(AccountError::CapacityExceeded);
        }
        let gid = self
            .groups
            .iter()
            .map(|group| group.gid)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(AccountError::CapacityExceeded)?;
        self.groups.push(GroupAccount {
            name: name.to_owned(),
            gid,
        });
        self.validate()?;
        Ok(gid)
    }

    pub fn add_user(
        &mut self,
        name: &str,
        primary_group: &str,
        home: &str,
    ) -> Result<u32, AccountError> {
        validate_name(name)?;
        validate_home(home)?;
        if self.users.iter().any(|user| user.name == name) {
            return Err(AccountError::NameInUse);
        }
        if self.users.len() >= MAX_USERS {
            return Err(AccountError::CapacityExceeded);
        }
        let primary_gid = self
            .group(primary_group)
            .map(|group| group.gid)
            .ok_or(AccountError::UnknownGroup)?;
        let uid = self
            .users
            .iter()
            .map(|user| user.uid)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(AccountError::CapacityExceeded)?;
        self.users.push(UserAccount {
            name: name.to_owned(),
            uid,
            primary_gid,
            supplementary_gids: Vec::new(),
            home: home.to_owned(),
            password_hash: None,
            disabled: false,
        });
        self.validate()?;
        Ok(uid)
    }

    pub fn set_password(&mut self, name: &str, password: &str) -> Result<(), AccountError> {
        let user = self
            .users
            .iter_mut()
            .find(|user| user.name == name)
            .ok_or(AccountError::UnknownUser)?;
        user.password_hash = Some(hash_password(password)?);
        user.disabled = false;
        self.validate()
    }

    pub fn set_supplementary_groups(
        &mut self,
        name: &str,
        group_names: &[String],
    ) -> Result<(), AccountError> {
        let gids = group_names
            .iter()
            .map(|group_name| {
                self.group(group_name)
                    .map(|group| group.gid)
                    .ok_or(AccountError::UnknownGroup)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let user = self
            .users
            .iter_mut()
            .find(|user| user.name == name)
            .ok_or(AccountError::UnknownUser)?;
        user.supplementary_gids = gids;
        self.validate()
    }

    pub fn file_permission(&self, path: &str, is_directory: bool) -> (u32, u32, u16) {
        self.files
            .iter()
            .find(|permission| permission.path.eq_ignore_ascii_case(path))
            .map(|permission| (permission.uid, permission.gid, permission.mode))
            .unwrap_or((
                0,
                0,
                if is_directory
                    || path.rsplit('/').next().is_some_and(|name| {
                        let name = name.to_ascii_lowercase();
                        name.ends_with(".bin") || name.ends_with(".img")
                    })
                {
                    0o755
                } else {
                    0o644
                },
            ))
    }

    pub fn can_access(
        &self,
        path: &str,
        is_directory: bool,
        uid: u32,
        gids: &[u32],
        required: u16,
    ) -> bool {
        if required == 0 || required & !0o7 != 0 {
            return false;
        }
        if uid == 0 {
            return true;
        }
        let (owner, group, mode) = self.file_permission(path, is_directory);
        let granted = if uid == owner {
            (mode >> 6) & 0o7
        } else if gids.contains(&group) {
            (mode >> 3) & 0o7
        } else {
            mode & 0o7
        };
        granted & required == required
    }

    pub fn set_file_mode(
        &mut self,
        path: &str,
        is_directory: bool,
        caller_uid: u32,
        mode: u16,
    ) -> Result<(), AccountError> {
        validate_metadata_path(path)?;
        if mode > 0o777 {
            return Err(AccountError::InvalidMetadata(
                "file mode must be between 000 and 777".to_owned(),
            ));
        }
        let index = self
            .files
            .iter()
            .position(|permission| permission.path.eq_ignore_ascii_case(path));
        if index.is_none() && self.files.len() >= MAX_FILE_PERMISSIONS {
            return Err(AccountError::CapacityExceeded);
        }
        let (uid, gid, _) = self.file_permission(path, is_directory);
        if caller_uid != 0 && uid != caller_uid {
            return Err(AccountError::PermissionDenied);
        }
        self.set_file_permission(path, uid, gid, mode)
    }

    pub fn set_file_owner(
        &mut self,
        path: &str,
        is_directory: bool,
        caller_uid: u32,
        new_uid: u32,
        new_gid: u32,
        caller_gids: &[u32],
    ) -> Result<(), AccountError> {
        validate_metadata_path(path)?;
        if self.users.iter().all(|user| user.uid != new_uid) {
            return Err(AccountError::UnknownUser);
        }
        if self.groups.iter().all(|group| group.gid != new_gid) {
            return Err(AccountError::UnknownGroup);
        }
        let (uid, _, mode) = self.file_permission(path, is_directory);
        if caller_uid != 0
            && (caller_uid != uid || new_uid != caller_uid || !caller_gids.contains(&new_gid))
        {
            return Err(AccountError::PermissionDenied);
        }
        self.set_file_permission(path, new_uid, new_gid, mode)
    }

    pub fn register_created_path(
        &mut self,
        path: &str,
        is_directory: bool,
        uid: u32,
        gid: u32,
    ) -> Result<(), AccountError> {
        validate_metadata_path(path)?;
        if self.users.iter().all(|user| user.uid != uid) {
            return Err(AccountError::UnknownUser);
        }
        if self.groups.iter().all(|group| group.gid != gid) {
            return Err(AccountError::UnknownGroup);
        }
        let mode = if is_directory { 0o755 } else { 0o644 };
        self.set_file_permission(path, uid, gid, mode)
    }

    pub fn remove_file_permission(&mut self, path: &str) -> Result<(), AccountError> {
        validate_metadata_path(path)?;
        self.files
            .retain(|permission| !permission.path.eq_ignore_ascii_case(path));
        self.validate()
    }

    pub fn rename_file_permission(
        &mut self,
        source: &str,
        destination: &str,
    ) -> Result<(), AccountError> {
        validate_metadata_path(source)?;
        validate_metadata_path(destination)?;
        if self
            .files
            .iter()
            .any(|permission| permission.path.eq_ignore_ascii_case(destination))
        {
            return Err(AccountError::FilePermissionAlreadyExists);
        }
        if let Some(permission) = self
            .files
            .iter_mut()
            .find(|permission| permission.path.eq_ignore_ascii_case(source))
        {
            permission.path = destination.to_owned();
        }
        self.validate()
    }

    fn set_file_permission(
        &mut self,
        path: &str,
        uid: u32,
        gid: u32,
        mode: u16,
    ) -> Result<(), AccountError> {
        if let Some(permission) = self
            .files
            .iter_mut()
            .find(|permission| permission.path.eq_ignore_ascii_case(path))
        {
            permission.uid = uid;
            permission.gid = gid;
            permission.mode = mode;
        } else {
            if self.files.len() >= MAX_FILE_PERMISSIONS {
                return Err(AccountError::CapacityExceeded);
            }
            self.files.push(FilePermission {
                path: path.to_owned(),
                uid,
                gid,
                mode,
            });
        }
        self.validate()
    }

    fn validate(&self) -> Result<(), AccountError> {
        if self.version != ACCOUNT_STORE_VERSION
            || self.users.is_empty()
            || self.users.len() > MAX_USERS
            || self.groups.is_empty()
            || self.groups.len() > MAX_GROUPS
            || self.files.len() > MAX_FILE_PERMISSIONS
        {
            return Err(AccountError::InvalidMetadata(
                "account metadata version or record count is invalid".to_owned(),
            ));
        }
        let mut user_names = HashSet::new();
        let mut uids = HashSet::new();
        let mut group_names = HashSet::new();
        let mut gids = HashSet::new();
        let mut file_paths = HashSet::new();
        for group in &self.groups {
            validate_name(&group.name)?;
            if !group_names.insert(group.name.as_str()) || !gids.insert(group.gid) {
                return Err(AccountError::InvalidMetadata(
                    "duplicate group name or ID".to_owned(),
                ));
            }
        }
        for user in &self.users {
            validate_name(&user.name)?;
            validate_home(&user.home)?;
            if !user_names.insert(user.name.as_str()) || !uids.insert(user.uid) {
                return Err(AccountError::InvalidMetadata(
                    "duplicate user name or ID".to_owned(),
                ));
            }
            if !gids.contains(&user.primary_gid) {
                return Err(AccountError::InvalidMetadata(
                    "user primary group is missing".to_owned(),
                ));
            }
            let mut memberships = HashSet::new();
            if user.supplementary_gids.iter().any(|gid| {
                !gids.contains(gid) || !memberships.insert(*gid) || *gid == user.primary_gid
            }) {
                return Err(AccountError::InvalidMetadata(
                    "invalid supplementary group membership".to_owned(),
                ));
            }
            if let Some(hash) = &user.password_hash {
                let parsed = PasswordHash::new(hash).map_err(|error| {
                    AccountError::InvalidMetadata(format!("invalid password hash: {error}"))
                })?;
                if parsed.algorithm.as_str() != "argon2id"
                    || parsed.params.get_decimal("m") != Some(MAX_ARGON2_MEMORY_KIB)
                    || parsed.params.get_decimal("t") != Some(MAX_ARGON2_ITERATIONS)
                    || parsed.params.get_decimal("p") != Some(1)
                {
                    return Err(AccountError::InvalidMetadata(
                        "password hash parameters are unsupported".to_owned(),
                    ));
                }
            }
        }
        for permission in &self.files {
            validate_metadata_path(&permission.path)?;
            if permission.mode > 0o777
                || !uids.contains(&permission.uid)
                || !gids.contains(&permission.gid)
                || !file_paths.insert(permission.path.to_ascii_lowercase())
            {
                return Err(AccountError::InvalidMetadata(
                    "invalid or duplicate file permission record".to_owned(),
                ));
            }
        }
        if self
            .users
            .iter()
            .filter(|user| user.uid == 0 && user.name == "root" && user.primary_gid == 0)
            .count()
            != 1
            || self
                .groups
                .iter()
                .filter(|group| group.gid == 0 && group.name == "root")
                .count()
                != 1
        {
            return Err(AccountError::InvalidMetadata(
                "root user and group must own ID zero".to_owned(),
            ));
        }
        Ok(())
    }
}

fn hash_password(password: &str) -> Result<String, AccountError> {
    if password.len() < 8
        || password.len() > MAX_PASSWORD_BYTES
        || password.chars().any(char::is_control)
    {
        return Err(AccountError::InvalidPassword);
    }
    let mut salt_bytes = [0; 16];
    OsRng
        .try_fill_bytes(&mut salt_bytes)
        .map_err(|error| AccountError::HashingFailed(error.to_string()))?;
    let salt = SaltString::encode_b64(&salt_bytes)
        .map_err(|error| AccountError::HashingFailed(error.to_string()))?;
    argon2()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| AccountError::HashingFailed(error.to_string()))
}

fn argon2() -> Argon2<'static> {
    let params = Params::new(MAX_ARGON2_MEMORY_KIB, MAX_ARGON2_ITERATIONS, 1, None)
        .expect("static Argon2id parameters are valid");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

fn validate_name(name: &str) -> Result<(), AccountError> {
    if name.is_empty()
        || name.len() > MAX_USERNAME_BYTES
        || !name.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || (index != 0 && matches!(byte, b'_' | b'-' | b'.'))
        })
    {
        return Err(AccountError::InvalidName);
    }
    Ok(())
}

fn validate_home(home: &str) -> Result<(), AccountError> {
    if home.is_empty()
        || home.len() > 128
        || home.starts_with('/')
        || home
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        || !home
            .bytes()
            .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.' | b'/'))
    {
        return Err(AccountError::InvalidHome);
    }

    Ok(())
}

fn validate_metadata_path(path: &str) -> Result<(), AccountError> {
    if path.is_empty()
        || path.len() > 128
        || path.starts_with('/')
        || path.contains('\\')
        || path
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        || path
            .bytes()
            .any(|byte| !byte.is_ascii_alphanumeric() && !b"_.-/".contains(&byte))
        || path
            .split('/')
            .any(|component| component.eq_ignore_ascii_case(".sisuos"))
    {
        return Err(AccountError::InvalidMetadata(
            "file permission path is invalid or reserved".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn account_disk() -> (std::path::PathBuf, crate::disk::FolderDisk) {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let parent = std::env::temp_dir().join(format!("sc16-accounts-{unique}"));
        let root = parent.join("DISK0001");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("disk.json"),
            r#"{"version":1,"id":"DISK0001","label":"Test","capacity_bytes":65536,"read_only":false}"#,
        )
        .unwrap();
        let disk = crate::disk::FolderDisk::open(&root).unwrap();
        (parent, disk)
    }

    #[test]
    fn first_boot_requires_an_explicit_root_password_and_persists_argon2id() {
        let mut store = AccountStore::first_boot();
        assert!(store.authenticate("root", "anything").is_none());
        assert!(matches!(
            store.initialize_root_password("short"),
            Err(AccountError::InvalidPassword)
        ));
        store.initialize_root_password("strong password").unwrap();
        assert!(store.authenticate("root", "strong password").is_some());
        assert!(store.authenticate("root", "incorrect").is_none());
        assert!(matches!(
            store.initialize_root_password("another password"),
            Err(AccountError::AlreadyInitialized)
        ));

        let encoded = store.encode().unwrap();
        let decoded = AccountStore::decode(&encoded).unwrap();
        assert_eq!(
            decoded.authenticate("root", "strong password").unwrap().uid,
            0
        );
    }

    #[test]
    fn account_metadata_rejects_bad_schema_hashes_and_group_references() {
        let mut store = AccountStore::first_boot();
        store.users[0].password_hash = Some("$argon2i$v=19$m=1,t=1,p=1$c2FsdA$ZGF0YQ".to_owned());
        assert!(matches!(
            AccountStore::decode(&serde_json::to_vec(&store).unwrap()),
            Err(AccountError::InvalidMetadata(_))
        ));

        let mut store = AccountStore::first_boot();
        store.users[0].primary_gid = 99;
        assert!(matches!(
            store.encode(),
            Err(AccountError::InvalidMetadata(_))
        ));
    }

    #[test]
    fn users_remain_locked_until_a_password_is_set_and_membership_is_validated() {
        let mut store = AccountStore::first_boot();
        let group = store.add_group("users").unwrap();
        let uid = store.add_user("alice", "users", "home/alice").unwrap();
        assert_eq!(uid, 1);
        assert!(store.authenticate("alice", "secret").is_none());
        store.set_password("alice", "secret123").unwrap();
        let user = store.authenticate("alice", "secret123").unwrap();
        assert_eq!(user.primary_gid, group);
        assert_eq!(user.home, "home/alice");
        assert!(matches!(
            store.add_user("invalid/name", "users", "home/bob"),
            Err(AccountError::InvalidName)
        ));
    }

    #[test]
    fn file_permissions_apply_owner_group_other_and_restrict_chown() {
        let mut store = AccountStore::first_boot();
        let group_gid = store.add_group("users").unwrap();
        let alice_uid = store.add_user("alice", "users", "home/alice").unwrap();
        let alice_gids = [group_gid];

        assert!(store.can_access("shared/readme", false, alice_uid, &alice_gids, 0o4));
        assert!(!store.can_access("shared/readme", false, alice_uid, &alice_gids, 0o2));
        assert!(store.can_access("bin/app.bin", false, alice_uid, &alice_gids, 0o5));
        assert!(!store.can_access("bin/app.bin", false, alice_uid, &alice_gids, 0o2));
        store
            .set_file_owner("home/alice/note.txt", false, 0, alice_uid, group_gid, &[0])
            .unwrap();
        assert!(store.can_access("home/alice/note.txt", false, alice_uid, &alice_gids, 0o6));
        store
            .set_file_mode("home/alice/note.txt", false, alice_uid, 0o600)
            .unwrap();
        assert!(store.can_access("home/alice/note.txt", false, alice_uid, &alice_gids, 0o6));
        assert!(!store.can_access("home/alice/note.txt", false, 2, &[0], 0o4));
        assert!(store.can_access("home/alice/note.txt", false, 0, &[0], 0o7));
        assert!(
            store
                .set_file_owner("home/alice/note.txt", false, alice_uid, 0, 0, &alice_gids)
                .is_err()
        );
        assert!(
            store
                .set_file_mode("home/alice/note.txt", false, 2, 0o777)
                .is_err()
        );
    }

    #[test]
    fn account_store_loads_first_boot_persists_and_fails_closed_on_corruption() {
        let (parent, disk) = account_disk();
        let mut store = AccountStore::load(&disk).unwrap();
        store.initialize_root_password("strong password").unwrap();
        store.save(&disk).unwrap();
        let restored = AccountStore::load(&disk).unwrap();
        assert!(restored.authenticate("root", "strong password").is_some());

        fs::write(disk.root().join(ACCOUNT_STORE_PATH), b"{corrupt").unwrap();
        assert!(matches!(
            AccountStore::load(&disk),
            Err(AccountError::InvalidMetadata(_))
        ));
        fs::remove_dir_all(parent).unwrap();
    }
}
