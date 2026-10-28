//! Implements the emulator-backed round-robin task scheduler.
//! Defines saved CPU contexts and task states, with activation, rescheduling,
//! blocking, wakeup, exit, and task-lifecycle validation.

use crate::cpu::{Flags, PrivilegeMode};

pub type TaskId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskState {
    Runnable,
    Running,
    Blocked,
    Exited,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskContext {
    pub id: TaskId,
    pub registers: [i64; 8],
    pub pc: u32,
    pub sp: u32,
    pub kernel_sp: u32,
    pub flags: Flags,
    pub privilege_mode: PrivilegeMode,
    pub interrupt_enabled: bool,
    pub mmu_enabled: bool,
    pub page_directory: u32,
    pub state: TaskState,
    pub exit_code: Option<i32>,
}

impl TaskContext {
    pub fn user(pc: u32, sp: u32, kernel_sp: u32, mmu_enabled: bool, page_directory: u32) -> Self {
        Self {
            id: 0,
            registers: [0; 8],
            pc,
            sp,
            kernel_sp,
            flags: Flags::default(),
            privilege_mode: PrivilegeMode::User,
            interrupt_enabled: true,
            mmu_enabled,
            page_directory,
            state: TaskState::Runnable,
            exit_code: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchedulerError {
    TaskIdExhausted,
    TaskNotFound(TaskId),
    TaskNotRunnable(TaskId),
    NoCurrentTask,
}

#[derive(Clone, Debug, Default)]
pub struct Scheduler {
    tasks: Vec<TaskContext>,
    current: Option<TaskId>,
    next_id: TaskId,
}

impl Scheduler {
    pub fn create_task(&mut self, mut context: TaskContext) -> Result<TaskId, SchedulerError> {
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(SchedulerError::TaskIdExhausted)?;
        context.id = id;
        context.state = TaskState::Runnable;
        context.exit_code = None;
        self.tasks.push(context);
        Ok(id)
    }

    pub fn current_task(&self) -> Option<TaskId> {
        self.current
    }

    pub fn task(&self, id: TaskId) -> Option<&TaskContext> {
        self.tasks.iter().find(|task| task.id == id)
    }

    pub fn tasks(&self) -> &[TaskContext] {
        &self.tasks
    }

    pub fn activate(&mut self, id: TaskId) -> Result<TaskContext, SchedulerError> {
        if self.current.is_some() {
            return Err(SchedulerError::TaskNotRunnable(id));
        }
        let task = self
            .tasks
            .iter_mut()
            .find(|task| task.id == id)
            .ok_or(SchedulerError::TaskNotFound(id))?;
        if task.state != TaskState::Runnable {
            return Err(SchedulerError::TaskNotRunnable(id));
        }
        task.state = TaskState::Running;
        self.current = Some(id);
        Ok(task.clone())
    }

    pub fn reschedule(
        &mut self,
        mut current_context: TaskContext,
        current_state: TaskState,
    ) -> Result<Option<TaskContext>, SchedulerError> {
        let current_id = self.current.ok_or(SchedulerError::NoCurrentTask)?;
        current_context.id = current_id;
        current_context.state = current_state;
        let current_task = self
            .tasks
            .iter_mut()
            .find(|task| task.id == current_id)
            .ok_or(SchedulerError::TaskNotFound(current_id))?;
        current_task.clone_from(&current_context);
        self.current = None;

        let current_index = self
            .tasks
            .iter()
            .position(|task| task.id == current_id)
            .ok_or(SchedulerError::TaskNotFound(current_id))?;
        for offset in 1..=self.tasks.len() {
            let index = (current_index + offset) % self.tasks.len();
            if self.tasks[index].state == TaskState::Runnable {
                self.tasks[index].state = TaskState::Running;
                self.current = Some(self.tasks[index].id);
                return Ok(Some(self.tasks[index].clone()));
            }
        }
        Ok(None)
    }

    pub fn wake(&mut self, id: TaskId) -> Result<(), SchedulerError> {
        let task = self
            .tasks
            .iter_mut()
            .find(|task| task.id == id)
            .ok_or(SchedulerError::TaskNotFound(id))?;
        if task.state == TaskState::Blocked {
            task.state = TaskState::Runnable;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user_context(pc: u32) -> TaskContext {
        TaskContext::user(pc, 0x7000, 0xf000, false, 0)
    }

    #[test]
    fn scheduler_round_robins_runnable_tasks_and_preserves_saved_context() {
        let mut scheduler = Scheduler::default();
        let first = scheduler.create_task(user_context(0x100)).unwrap();
        let second = scheduler.create_task(user_context(0x200)).unwrap();
        let mut current = scheduler.activate(first).unwrap();
        current.registers[2] = 0x1234;
        current.flags.carry = true;
        let next = scheduler
            .reschedule(current, TaskState::Runnable)
            .unwrap()
            .unwrap();

        assert_eq!(next.id, second);
        assert_eq!(next.pc, 0x200);
        let next = scheduler
            .reschedule(next, TaskState::Runnable)
            .unwrap()
            .unwrap();
        assert_eq!(next.id, first);
        assert_eq!(next.registers[2], 0x1234);
        assert!(next.flags.carry);
    }

    #[test]
    fn scheduler_skips_blocked_and_exited_tasks_and_wakes_blocked_tasks() {
        let mut scheduler = Scheduler::default();
        let first = scheduler.create_task(user_context(0x100)).unwrap();
        let second = scheduler.create_task(user_context(0x200)).unwrap();
        let third = scheduler.create_task(user_context(0x300)).unwrap();
        let current = scheduler.activate(first).unwrap();
        let next = scheduler
            .reschedule(current, TaskState::Blocked)
            .unwrap()
            .unwrap();
        assert_eq!(next.id, second);
        let next = scheduler
            .reschedule(next, TaskState::Exited)
            .unwrap()
            .unwrap();
        assert_eq!(next.id, third);
        assert_eq!(scheduler.task(first).unwrap().state, TaskState::Blocked);
        scheduler.wake(first).unwrap();
        let next = scheduler
            .reschedule(next, TaskState::Runnable)
            .unwrap()
            .unwrap();
        assert_eq!(next.id, first);
    }
}
