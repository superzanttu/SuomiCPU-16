//! Defines the emulator-backed guest syscall ABI and kernel bookkeeping.
//! Contains syscall numbers, register/error conventions, file handles, process records,
//! sleep/wait state, and semaphores used by CPU syscall dispatch.

use std::collections::{HashMap, VecDeque};
use std::convert::TryFrom;

use crate::scheduler::TaskId;

pub const ABI_VERSION: i64 = 1;
pub const MAX_TRANSFER_BYTES: usize = 4096;

pub const REGISTER_NUMBER: usize = 0;
pub const REGISTER_ARGUMENT_0: usize = 1;
pub const REGISTER_ARGUMENT_1: usize = 2;
pub const REGISTER_ARGUMENT_2: usize = 3;
pub const REGISTER_ARGUMENT_3: usize = 4;
pub const REGISTER_ARGUMENT_4: usize = 5;
pub const REGISTER_ARGUMENT_5: usize = 6;
pub const REGISTER_ABI_VERSION: usize = 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i64)]
pub enum SyscallNumber {
    GetPid = 0,
    Exit = 1,
    Yield = 2,
    Spawn = 3,
    Mmap = 4,
    Open = 5,
    Read = 6,
    Write = 7,
    Close = 8,
    Sleep = 9,
    Wait = 10,
    Semaphore = 11,
    Seek = 12,
    ScNet = 13,
    ProcessInfo = 14,
    StartupArgument = 15,
    MonotonicMillis = 16,
    SpawnArgument = 17,
    TryWait = 18,
    SleepMillis = 19,
    ProcessStatus = 20,
}

impl TryFrom<i64> for SyscallNumber {
    type Error = ();

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::GetPid),
            1 => Ok(Self::Exit),
            2 => Ok(Self::Yield),
            3 => Ok(Self::Spawn),
            4 => Ok(Self::Mmap),
            5 => Ok(Self::Open),
            6 => Ok(Self::Read),
            7 => Ok(Self::Write),
            8 => Ok(Self::Close),
            9 => Ok(Self::Sleep),
            10 => Ok(Self::Wait),
            11 => Ok(Self::Semaphore),
            12 => Ok(Self::Seek),
            13 => Ok(Self::ScNet),
            14 => Ok(Self::ProcessInfo),
            15 => Ok(Self::StartupArgument),
            16 => Ok(Self::MonotonicMillis),
            17 => Ok(Self::SpawnArgument),
            18 => Ok(Self::TryWait),
            19 => Ok(Self::SleepMillis),
            20 => Ok(Self::ProcessStatus),
            _ => Err(()),
        }
    }
}

pub mod error {
    pub const NO_ENTRY: i64 = -2;
    pub const INTERRUPTED: i64 = -4;
    pub const TRY_AGAIN: i64 = -11;
    pub const IO: i64 = -5;
    pub const BAD_HANDLE: i64 = -9;
    pub const NO_MEMORY: i64 = -12;
    pub const PERMISSION: i64 = -13;
    pub const BAD_ADDRESS: i64 = -14;
    pub const BUSY: i64 = -16;
    pub const INVALID: i64 = -22;
    pub const TOO_MANY_HANDLES: i64 = -24;
    pub const NO_CHILD: i64 = -10;
    pub const NOT_IMPLEMENTED: i64 = -38;
    pub const OVERFLOW: i64 = -75;
}

pub mod open_flags {
    pub const READ: u32 = 1;
    pub const WRITE: u32 = 2;
    pub const CREATE: u32 = 4;
    pub const TRUNCATE: u32 = 8;
    pub const APPEND: u32 = 16;
    pub const ALL: u32 = READ | WRITE | CREATE | TRUNCATE | APPEND;
}

pub mod semaphore_operation {
    pub const CREATE: u32 = 0;
    pub const WAIT: u32 = 1;
    pub const POST: u32 = 2;
    pub const DESTROY: u32 = 3;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OpenFile {
    pub path: String,
    pub offset: u64,
    pub flags: u32,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ProcessRecord {
    pub name: String,
    pub startup_argument: u32,
    pub parent: Option<TaskId>,
    pub children: Vec<TaskId>,
    pub exit_code: Option<i32>,
    pub waiters: VecDeque<TaskId>,
    pub next_mmap: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct Semaphore {
    pub owner: TaskId,
    pub count: u32,
    pub waiters: VecDeque<TaskId>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct KernelState {
    pub ticks: u64,
    pub processes: HashMap<TaskId, ProcessRecord>,
    pub open_files: HashMap<TaskId, HashMap<u32, OpenFile>>,
    pub sleeping: HashMap<TaskId, u64>,
    pub sleeping_millis: HashMap<TaskId, (u32, u32)>,
    pub waiting_for: HashMap<TaskId, TaskId>,
    pub semaphores: HashMap<u32, Semaphore>,
    pub next_semaphore: u32,
    pub pending_results: HashMap<TaskId, i64>,
    pub pending_wait_status: HashMap<TaskId, (TaskId, i32)>,
}

impl KernelState {
    pub fn register_process(&mut self, id: TaskId, parent: Option<TaskId>) {
        self.processes.entry(id).or_insert_with(|| ProcessRecord {
            parent,
            next_mmap: 0x2000_0000,
            ..ProcessRecord::default()
        });
        if let Some(parent) = parent {
            let record = self.processes.entry(parent).or_default();
            if !record.children.contains(&id) {
                record.children.push(id);
            }
        }
    }

    pub fn allocate_file_handle(&mut self, id: TaskId, file: OpenFile) -> Result<u32, i64> {
        let files = self.open_files.entry(id).or_default();
        let handle = (3..u32::MAX)
            .find(|handle| !files.contains_key(handle))
            .ok_or(error::TOO_MANY_HANDLES)?;
        files.insert(handle, file);
        Ok(handle)
    }

    pub fn file(&self, id: TaskId, handle: u32) -> Option<&OpenFile> {
        self.open_files.get(&id)?.get(&handle)
    }

    pub fn file_mut(&mut self, id: TaskId, handle: u32) -> Option<&mut OpenFile> {
        self.open_files.get_mut(&id)?.get_mut(&handle)
    }

    pub fn close_file(&mut self, id: TaskId, handle: u32) -> bool {
        self.open_files
            .get_mut(&id)
            .and_then(|files| files.remove(&handle))
            .is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syscall_numbers_are_stable_and_unknown_values_are_rejected() {
        assert_eq!(
            SyscallNumber::try_from(SyscallNumber::Seek as i64),
            Ok(SyscallNumber::Seek)
        );
        assert_eq!(SyscallNumber::try_from(13), Ok(SyscallNumber::ScNet));
        assert_eq!(SyscallNumber::try_from(-1), Err(()));
        assert_eq!(SyscallNumber::try_from(14), Ok(SyscallNumber::ProcessInfo));
        assert_eq!(SyscallNumber::try_from(19), Ok(SyscallNumber::SleepMillis));
        assert_eq!(SyscallNumber::try_from(20), Ok(SyscallNumber::ProcessStatus));
        assert_eq!(SyscallNumber::try_from(21), Err(()));
    }

    #[test]
    fn file_handles_are_process_local_and_start_after_standard_handles() {
        let mut state = KernelState::default();
        let file = OpenFile {
            path: "data.txt".to_owned(),
            offset: 0,
            flags: open_flags::READ,
        };
        let handle = state.allocate_file_handle(7, file.clone()).unwrap();
        assert_eq!(handle, 3);
        assert_eq!(state.file(7, handle), Some(&file));
        assert_eq!(state.file(8, handle), None);
        assert!(state.close_file(7, handle));
        assert_eq!(state.file(7, handle), None);
    }

    #[test]
    fn process_registration_records_parent_and_children() {
        let mut state = KernelState::default();
        state.register_process(1, None);
        state.register_process(2, Some(1));
        assert_eq!(state.processes[&2].parent, Some(1));
        assert_eq!(state.processes[&1].children, [2]);
    }
}
