use crate::{Capability, Instruction, InstructionError, Opcode, Permissions, Tag, Word};
use serde::Serialize;
use std::fmt;

const INSTRUCTION_WORDS: usize = 0x10000;
const DATA_WORDS: usize = 0x8000;

const TAG_TRAP: u8 = 0x01;
const POISON_TRAP: u8 = 0x02;
const OVERFLOW_TRAP: u8 = 0x03;
const DIV_ZERO_TRAP: u8 = 0x04;
const BAD_OP_TRAP: u8 = 0x0b;

#[derive(Clone, Copy)]
struct Fault {
    cause: u8,
    tval: Word,
}

impl Fault {
    const fn new(cause: u8, tval: Word) -> Self {
        Self { cause, tval }
    }
}

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
    tcause: u8,
    tpc: u32,
    tval: Word,
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
            tcause: 0,
            tpc: 0,
            tval: Word::unit(),
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
        let instruction = match Instruction::decode(insn) {
            Ok(instruction) => instruction,
            Err(_) => {
                self.pc = self.pc.wrapping_add(1);
                self.trap(BAD_OP_TRAP, Word::int(insn as i32), pc);
                return Ok(self.trace(pc, insn, None));
            }
        };
        self.pc = self.pc.wrapping_add(1);
        let result = self.execute(instruction, pc);
        match result {
            Ok(mem_write) => Ok(self.trace(pc, insn, mem_write)),
            Err(fault) => {
                self.trap(fault.cause, fault.tval, pc);
                Ok(self.trace(pc, insn, None))
            }
        }
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

    pub const fn tcause(&self) -> u8 {
        self.tcause
    }

    pub const fn tpc(&self) -> u32 {
        self.tpc
    }

    pub const fn tval(&self) -> Word {
        self.tval
    }

    fn execute(
        &mut self,
        instruction: Instruction,
        pc: u32,
    ) -> Result<Option<(u32, Word)>, Fault> {
        match instruction.opcode() {
            Opcode::Nop => Ok(None),
            Opcode::Halt => {
                self.status = MachineStatus::Halted {
                    exit_code: self.regs[1].payload(),
                };
                Ok(None)
            }
            Opcode::Mov => {
                let value = self.read_reg(instruction.rs1())?;
                self.write_reg(instruction.rd(), value);
                Ok(None)
            }
            Opcode::MovI => {
                self.write_reg(instruction.rd(), Word::int(instruction.imm18()));
                Ok(None)
            }
            Opcode::MovH => {
                let high = (u32::from(instruction.imm14_bits()) & 0x3fff) << 18;
                let low = self.regs[instruction.rd() as usize].payload() & 0x0003_ffff;
                self.write_reg(instruction.rd(), Word::int((high | low) as i32));
                Ok(None)
            }
            Opcode::Poison => {
                self.write_reg(instruction.rd(), Word::poison());
                Ok(None)
            }
            Opcode::Mktag => {
                if instruction.eff() != 9 {
                    return Err(Fault::new(BAD_OP_TRAP, Word::int(instruction.eff() as i32)));
                }
                let source = self.read_reg(instruction.rs1())?;
                let tag = Tag::try_from((instruction.imm14_bits() & 0x0f) as u8)
                    .map_err(|_| Fault::new(TAG_TRAP, Word::int(instruction.imm14_bits() as i32)))?;
                let value = Word::try_new(tag, source.payload())
                    .map_err(|_| Fault::new(TAG_TRAP, source))?;
                self.write_reg(instruction.rd(), value);
                Ok(None)
            }
            Opcode::Add => self.binary_int(instruction, |left, right| {
                left.checked_add(right).ok_or(OVERFLOW_TRAP)
            }),
            Opcode::Sub => self.binary_int(instruction, |left, right| {
                left.checked_sub(right).ok_or(OVERFLOW_TRAP)
            }),
            Opcode::Mul => self.binary_int(instruction, |left, right| {
                left.checked_mul(right).ok_or(OVERFLOW_TRAP)
            }),
            Opcode::Div => self.binary_int(instruction, |left, right| {
                if right == 0 {
                    return Err(DIV_ZERO_TRAP);
                }
                left.checked_div(right).ok_or(OVERFLOW_TRAP)
            }),
            Opcode::Rem => self.binary_int(instruction, |left, right| {
                if right == 0 {
                    return Err(DIV_ZERO_TRAP);
                }
                left.checked_rem(right).ok_or(OVERFLOW_TRAP)
            }),
            Opcode::And => self.binary_payload(instruction, |left, right| left & right),
            Opcode::Or => self.binary_payload(instruction, |left, right| left | right),
            Opcode::Xor => self.binary_payload(instruction, |left, right| left ^ right),
            Opcode::Shl => self.binary_shift(instruction, |left, right| left << right),
            Opcode::Shr => self.binary_shift(instruction, |left, right| left >> right),
            Opcode::Sar => self.binary_shift(instruction, |left, right| {
                ((left as i32) >> right) as u32
            }),
            Opcode::AddI => self.immediate_int(instruction, |left, immediate| {
                left.checked_add(immediate).ok_or(OVERFLOW_TRAP)
            }),
            Opcode::AndI => self.immediate_payload(instruction, |left, immediate| {
                left & immediate as u32
            }),
            Opcode::OrI => self.immediate_payload(instruction, |left, immediate| {
                left | immediate as u32
            }),
            Opcode::XorI => self.immediate_payload(instruction, |left, immediate| {
                left ^ immediate as u32
            }),
            Opcode::ShlI => self.immediate_shift(instruction, |left, immediate| left << immediate),
            Opcode::ShrI => self.immediate_shift(instruction, |left, immediate| left >> immediate),
            Opcode::Cmp => self.compare(instruction),
            Opcode::BNot => {
                let value = self.read_bool(instruction.rs1())?;
                self.write_reg(instruction.rd(), Word::bool(!value));
                Ok(None)
            }
            Opcode::BAnd => self.binary_bool(instruction, |left, right| left && right),
            Opcode::BOr => self.binary_bool(instruction, |left, right| left || right),
            Opcode::Tag => {
                let value = self.regs[instruction.rs1() as usize];
                self.write_reg(instruction.rd(), Word::int(value.tag() as i32));
                Ok(None)
            }
            Opcode::IsTag => {
                let value = self.regs[instruction.rs1() as usize];
                let tag = (instruction.imm14_bits() & 0x0f) as u8;
                self.write_reg(
                    instruction.rd(),
                    Word::bool(value.tag() as u8 == tag),
                );
                Ok(None)
            }
            _ => Err(Fault::new(BAD_OP_TRAP, Word::int(pc as i32))),
        }
    }

    fn binary_int<F>(&mut self, instruction: Instruction, operation: F) -> Result<Option<(u32, Word)>, Fault>
    where
        F: FnOnce(i32, i32) -> Result<i32, u8>,
    {
        let left = self.read_int(instruction.rs1())?;
        let right = self.read_int(instruction.rs2())?;
        let result = operation(left, right).map_err(|cause| Fault::new(cause, Word::int(left)))?;
        self.write_reg(instruction.rd(), Word::int(result));
        Ok(None)
    }

    fn binary_payload<F>(&mut self, instruction: Instruction, operation: F) -> Result<Option<(u32, Word)>, Fault>
    where
        F: FnOnce(u32, u32) -> u32,
    {
        let left = self.read_int(instruction.rs1())? as u32;
        let right = self.read_int(instruction.rs2())? as u32;
        self.write_reg(instruction.rd(), Word::int(operation(left, right) as i32));
        Ok(None)
    }

    fn binary_shift<F>(&mut self, instruction: Instruction, operation: F) -> Result<Option<(u32, Word)>, Fault>
    where
        F: FnOnce(u32, u32) -> u32,
    {
        let left = self.read_int(instruction.rs1())? as u32;
        let right = self.read_int(instruction.rs2())?;
        let shift = u32::try_from(right).map_err(|_| Fault::new(OVERFLOW_TRAP, Word::int(right)))?;
        if shift >= 32 {
            return Err(Fault::new(OVERFLOW_TRAP, Word::int(right as i32)));
        }
        self.write_reg(instruction.rd(), Word::int(operation(left, shift) as i32));
        Ok(None)
    }

    fn immediate_int<F>(&mut self, instruction: Instruction, operation: F) -> Result<Option<(u32, Word)>, Fault>
    where
        F: FnOnce(i32, i32) -> Result<i32, u8>,
    {
        let left = self.read_int(instruction.rs1())?;
        let result = operation(left, instruction.imm14())
            .map_err(|cause| Fault::new(cause, Word::int(left)))?;
        self.write_reg(instruction.rd(), Word::int(result));
        Ok(None)
    }

    fn immediate_payload<F>(&mut self, instruction: Instruction, operation: F) -> Result<Option<(u32, Word)>, Fault>
    where
        F: FnOnce(u32, i32) -> u32,
    {
        let left = self.read_int(instruction.rs1())? as u32;
        self.write_reg(
            instruction.rd(),
            Word::int(operation(left, instruction.imm14()) as i32),
        );
        Ok(None)
    }

    fn immediate_shift<F>(&mut self, instruction: Instruction, operation: F) -> Result<Option<(u32, Word)>, Fault>
    where
        F: FnOnce(u32, u32) -> u32,
    {
        let left = self.read_int(instruction.rs1())? as u32;
        let immediate = instruction.imm14();
        if !(0..32).contains(&immediate) {
            return Err(Fault::new(OVERFLOW_TRAP, Word::int(immediate)));
        }
        self.write_reg(
            instruction.rd(),
            Word::int(operation(left, immediate as u32) as i32),
        );
        Ok(None)
    }

    fn binary_bool<F>(&mut self, instruction: Instruction, operation: F) -> Result<Option<(u32, Word)>, Fault>
    where
        F: FnOnce(bool, bool) -> bool,
    {
        let left = self.read_bool(instruction.rs1())?;
        let right = self.read_bool(instruction.rs2())?;
        self.write_reg(instruction.rd(), Word::bool(operation(left, right)));
        Ok(None)
    }

    fn compare(&mut self, instruction: Instruction) -> Result<Option<(u32, Word)>, Fault> {
        let left = self.read_reg(instruction.rs1())?;
        let right = self.read_reg(instruction.rs2())?;
        if left.tag() != right.tag() {
            return Err(Fault::new(TAG_TRAP, right));
        }
        let condition = (instruction.aux10_bits() & 0x07) as u8;
        let result = match condition {
            0 => left.payload() == right.payload(),
            1 => left.payload() != right.payload(),
            2..=5 if matches!(left.tag(), crate::Tag::Int | crate::Tag::Char) => {
                let left = left.payload();
                let right = right.payload();
                match condition {
                    2 => (left as i32) < (right as i32),
                    3 => (left as i32) <= (right as i32),
                    4 => (left as i32) > (right as i32),
                    _ => (left as i32) >= (right as i32),
                }
            }
            6 | 7 if matches!(left.tag(), crate::Tag::Int | crate::Tag::Char) => {
                match condition {
                    6 => left.payload() < right.payload(),
                    _ => left.payload() <= right.payload(),
                }
            }
            _ => return Err(Fault::new(TAG_TRAP, left)),
        };
        self.write_reg(instruction.rd(), Word::bool(result));
        Ok(None)
    }

    fn read_reg(&self, index: u8) -> Result<Word, Fault> {
        let value = self.regs[index as usize];
        if value.tag() == crate::Tag::Poison {
            Err(Fault::new(POISON_TRAP, value))
        } else {
            Ok(value)
        }
    }

    fn read_int(&self, index: u8) -> Result<i32, Fault> {
        let value = self.read_reg(index)?;
        value
            .as_i32()
            .map_err(|_| Fault::new(TAG_TRAP, value))
    }

    fn read_bool(&self, index: u8) -> Result<bool, Fault> {
        let value = self.read_reg(index)?;
        if value.tag() != crate::Tag::Bool {
            return Err(Fault::new(TAG_TRAP, value));
        }
        Ok(value.payload() != 0)
    }

    fn write_reg(&mut self, index: u8, value: Word) {
        if index != 0 {
            self.regs[index as usize] = value;
        }
    }

    fn trap(&mut self, cause: u8, tval: Word, tpc: u32) {
        self.tcause = cause;
        self.tpc = tpc;
        self.tval = tval;
        self.status = MachineStatus::Trapped { cause, tpc, tval };
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
    BadPc(u32),
    Stopped,
}

impl fmt::Display for MachineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProgramTooLarge(words) => write!(formatter, "program has {words} words; maximum is 65536"),
            Self::Capability(error) => write!(formatter, "invalid boot capability: {error}"),
            Self::Decode(error) => write!(formatter, "cannot decode instruction: {error}"),
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
