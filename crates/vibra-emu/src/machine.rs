use crate::{Capability, Permissions, Word};
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
        })
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
}

#[derive(Debug)]
pub enum MachineError {
    ProgramTooLarge(usize),
    Capability(crate::CapabilityError),
}

impl fmt::Display for MachineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProgramTooLarge(words) => write!(formatter, "program has {words} words; maximum is 65536"),
            Self::Capability(error) => write!(formatter, "invalid boot capability: {error}"),
        }
    }
}

impl std::error::Error for MachineError {}
