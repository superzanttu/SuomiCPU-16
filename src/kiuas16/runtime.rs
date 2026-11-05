use std::fmt;
use std::io::{self, Write};

use super::{CoreTrap, Kiuas16Machine, PhysicalAccessError, StepOutcome};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RunReport {
    pub steps: u64,
    pub retired: u64,
    pub traps: u64,
    pub interrupts: u64,
    pub waiting: u64,
    pub last_trap: Option<CoreTrap>,
    pub input_accepted: usize,
    pub output_bytes: u64,
}

#[derive(Debug)]
pub enum RunError {
    InvalidStepBudget,
    Core(CoreTrap),
    Input(PhysicalAccessError),
    Media(io::Error),
    Output(io::Error),
}

impl fmt::Display for RunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidStepBudget => write!(formatter, "step budget must be nonzero"),
            Self::Core(trap) => write!(formatter, "unhandled core trap: {trap:?}"),
            Self::Input(error) => write!(formatter, "serial input failed: {error:?}"),
            Self::Media(error) => write!(formatter, "root media I/O failed: {error}"),
            Self::Output(error) => write!(formatter, "serial output failed: {error}"),
        }
    }
}

impl std::error::Error for RunError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Media(error) | Self::Output(error) => Some(error),
            _ => None,
        }
    }
}

pub fn run_headless(
    machine: &mut Kiuas16Machine,
    steps: u64,
    input: &[u8],
    output: &mut impl Write,
) -> Result<RunReport, RunError> {
    if steps == 0 {
        return Err(RunError::InvalidStepBudget);
    }
    let mut report = RunReport::default();
    for _ in 0..steps {
        let remaining = &input[report.input_accepted..];
        let count = remaining.len().min(machine.bus().uart_input_capacity());
        if count != 0 {
            report.input_accepted += machine
                .receive_uart_input(&remaining[..count])
                .map_err(RunError::Input)?;
        }
        match machine.step().map_err(RunError::Core)? {
            StepOutcome::Retired => report.retired += 1,
            StepOutcome::TrapDelivered(trap) => {
                report.traps += 1;
                report.last_trap = Some(trap);
            }
            StepOutcome::InterruptDelivered(_) => report.interrupts += 1,
            StepOutcome::WaitingForInterrupt => report.waiting += 1,
        }
        report.steps += 1;
        let bytes = machine.take_uart_output();
        output.write_all(&bytes).map_err(RunError::Output)?;
        report.output_bytes += bytes.len() as u64;
        if let Some(error) = machine.bus_mut().take_root_block_io_error() {
            return Err(RunError::Media(error));
        }
    }
    output.flush().map_err(RunError::Output)?;
    Ok(report)
}
