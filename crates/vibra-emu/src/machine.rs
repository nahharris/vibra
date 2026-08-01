use crate::{Capability, Instruction, InstructionError, Opcode, Permissions, Word};
use serde::Serialize;
use std::fmt;

const INSTRUCTION_WORDS: usize = 0x10000;
const DATA_WORDS: usize = 0x8000;

pub struct Machine {
    pc: u32,
    emr: u16,
    hp: u32,
    fsp: u16,
    pcc: Capability,
    hpc: Capability,
    regs: [Word; 16],
    caps: [Capability; 8],
    instruction_memory: Vec<u32>,
    data_memory: Vec<Word>,
    status: MachineStatus,
}

impl Machine {
    pub fn boot(program: &[u32]) -> Result<Self, MachineError> {
        if program.len() > INSTRUCTION_WORDS {
            return Err(MachineError::ProgramTooLarge(program.len()));
        }

        let pcc = Capability::new(0, 0x10000, Permissions::EXECUTE | Permissions::DERIVE, 0)
            .map_err(MachineError::Capability)?;
        let hpc = Capability::new(
            0x4000,
            0x4000,
            Permissions::READ
                | Permissions::WRITE
                | Permissions::ALLOCATE
                | Permissions::DERIVE,
            0,
        )
        .map_err(MachineError::Capability)?;
        let static_data = Capability::new(
            0,
            0x4000,
            Permissions::READ | Permissions::WRITE | Permissions::DERIVE,
            0,
        )
        .map_err(MachineError::Capability)?;
        let devices = Capability::new(
            0xF00000,
            0x100,
            Permissions::READ | Permissions::WRITE | Permissions::MMIO | Permissions::DERIVE,
            0,
        )
        .map_err(MachineError::Capability)?;

        let mut instruction_memory = vec![0; INSTRUCTION_WORDS];
        instruction_memory[..program.len()].copy_from_slice(program);
        let mut regs = [Word::poison(); 16];
        regs[0] = Word::int(0);
        let mut caps = [Capability::null(); 8];
        caps[1] = static_data;
        caps[2] = devices;

        Ok(Self {
            pc: 0,
            emr: 0xffff,
            hp: 0x4000,
            fsp: 0,
            pcc,
            hpc,
            regs,
            caps,
            instruction_memory,
            data_memory: vec![Word::poison(); DATA_WORDS],
            status: MachineStatus::Running,
        })
    }

    pub fn step(&mut self) -> Result<Trace, MachineError> {
        if !self.status.is_running() {
            return Err(MachineError::Stopped);
        }
        let pc = self.pc;
        let insn = *self
            .instruction_memory
            .get(pc as usize)
            .ok_or(MachineError::BadPc(pc))?;
        let instruction = Instruction::decode(insn).map_err(MachineError::Decode)?;
        self.pc = self.pc.wrapping_add(1);
        match instruction.opcode() {
            Opcode::Nop => {}
            Opcode::Halt => {
                self.status = MachineStatus::Halted {
                    exit_code: self.regs[1].payload(),
                };
            }
            _ => return Err(MachineError::UnsupportedOpcode(instruction.opcode())),
        }
        Ok(self.trace(pc, insn, None))
    }

    pub fn run(&mut self, max_steps: u64) -> Result<RunReport, MachineError> {
        let mut traces = Vec::new();
        while self.status.is_running() && traces.len() < max_steps as usize {
            traces.push(self.step()?);
        }
        Ok(RunReport {
            status: self.status,
            steps: traces.len() as u64,
            step_limit_reached: self.status.is_running() && traces.len() >= max_steps as usize,
            traces,
        })
    }

    pub const fn status(&self) -> &MachineStatus {
        &self.status
    }

    pub const fn pc(&self) -> u32 {
        self.pc
    }

    pub const fn emr(&self) -> u16 {
        self.emr
    }

    pub const fn hp(&self) -> u32 {
        self.hp
    }

    pub const fn fsp(&self) -> u16 {
        self.fsp
    }

    pub const fn reg(&self, index: usize) -> Word {
        self.regs[index]
    }

    pub const fn cap(&self, index: usize) -> Capability {
        self.caps[index]
    }

    pub const fn pcc(&self) -> Capability {
        self.pcc
    }

    pub const fn hpc(&self) -> Capability {
        self.hpc
    }

    fn trace(&self, pc: u32, insn: u32, mem_write: Option<(u32, Word)>) -> Trace {
        Trace {
            pc,
            insn,
            regs: self.regs,
            caps: self.caps,
            emr: self.emr,
            hp: self.hp,
            fsp: self.fsp,
            mem_write,
            trap: self.status.trap_cause(),
        }
    }
}

#[derive(Debug)]
pub enum MachineError {
    ProgramTooLarge(usize),
    Capability(crate::CapabilityError),
    Decode(InstructionError),
    UnsupportedOpcode(Opcode),
    BadPc(u32),
    Stopped,
}

impl fmt::Display for MachineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProgramTooLarge(words) => write!(formatter, "program has {words} words; maximum is 65536"),
            Self::Capability(error) => write!(formatter, "invalid boot capability: {error}"),
            Self::Decode(error) => write!(formatter, "cannot decode instruction: {error}"),
            Self::UnsupportedOpcode(opcode) => write!(formatter, "opcode {opcode:?} is not implemented yet"),
            Self::BadPc(pc) => write!(formatter, "instruction fetch outside memory at PC 0x{pc:08x}"),
            Self::Stopped => write!(formatter, "machine has already halted or trapped"),
        }
    }
}

impl std::error::Error for MachineError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum MachineStatus {
    Running,
    Halted { exit_code: u32 },
    Trapped { cause: u8, tpc: u32, tval: Word },
}

impl MachineStatus {
    pub const fn is_running(self) -> bool {
        matches!(self, Self::Running)
    }

    pub const fn is_halted(self) -> bool {
        matches!(self, Self::Halted { .. })
    }

    pub const fn exit_code(self) -> Option<u32> {
        match self {
            Self::Halted { exit_code } => Some(exit_code),
            Self::Running | Self::Trapped { .. } => None,
        }
    }

    pub const fn trap_cause(self) -> Option<u8> {
        match self {
            Self::Trapped { cause, .. } => Some(cause),
            Self::Running | Self::Halted { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Trace {
    pub pc: u32,
    pub insn: u32,
    pub regs: [Word; 16],
    pub caps: [Capability; 8],
    pub emr: u16,
    pub hp: u32,
    pub fsp: u16,
    pub mem_write: Option<(u32, Word)>,
    pub trap: Option<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RunReport {
    pub status: MachineStatus,
    pub steps: u64,
    pub step_limit_reached: bool,
    pub traces: Vec<Trace>,
}
