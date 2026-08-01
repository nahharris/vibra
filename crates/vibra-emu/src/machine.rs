use crate::{Capability, Instruction, Opcode, Permissions, Tag, Word};
use serde::Serialize;
use std::fmt;

pub const TAG_TRAP: u8 = 0x01;
pub const POISON_TRAP: u8 = 0x02;
pub const OVERFLOW_TRAP: u8 = 0x03;
pub const DIV_ZERO_TRAP: u8 = 0x04;
pub const CAP_BOUND_TRAP: u8 = 0x05;
pub const CAP_PERM_TRAP: u8 = 0x06;
pub const EFFECT_TRAP: u8 = 0x07;
pub const HEAP_TRAP: u8 = 0x08;
pub const FRAME_TRAP: u8 = 0x09;
pub const SEAL_TRAP: u8 = 0x0a;
pub const BAD_OP_TRAP: u8 = 0x0b;
pub const BAD_PC_TRAP: u8 = 0x0c;

const INSTRUCTION_WORDS: usize = 0x10000;
const DATA_WORDS: usize = 0x8000;
const MMIO_BASE: u32 = 0x00f0_0000;
const MMIO_WORDS: usize = 0x100;
const FRAME_LIMIT: usize = 1024;

const UART_TX: usize = 0x00;
const UART_RX: usize = 0x01;
const UART_STAT: usize = 0x02;
const LED: usize = 0x10;
const BUTTON: usize = 0x11;
const CYCLE_LO: usize = 0x20;
const CYCLE_HI: usize = 0x21;
const RNG: usize = 0x30;

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

#[derive(Clone, Copy)]
enum Frame {
    Call {
        return_pc: u32,
        emr: u16,
        pcc: Capability,
    },
    Effect {
        emr: u16,
    },
}

#[derive(Clone, Copy)]
struct SealedWord {
    token: u32,
    otype: u8,
    value: Word,
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
    mmio: [Word; MMIO_WORDS],
    uart_output: Vec<u8>,
    cycles: u64,
    rng_state: u32,
    frames: Vec<Frame>,
    sealed_words: Vec<SealedWord>,
    next_seal_token: u32,
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
            Permissions::READ | Permissions::WRITE | Permissions::ALLOCATE | Permissions::DERIVE,
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
            MMIO_BASE,
            MMIO_WORDS as u32,
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
        let mut mmio = [Word::int(0); MMIO_WORDS];
        mmio[UART_RX] = Word::int(0);
        mmio[UART_STAT] = Word::int(1);
        mmio[LED] = Word::int(0);
        mmio[BUTTON] = Word::int(0);

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
            mmio,
            uart_output: Vec::new(),
            cycles: 0,
            rng_state: 0x1234_5678,
            frames: Vec::new(),
            sealed_words: Vec::new(),
            next_seal_token: 1,
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

        self.cycles = self.cycles.wrapping_add(1);
        let pc = self.pc;
        if !self.code_address_valid(pc, self.pcc) {
            self.trap(BAD_PC_TRAP, Word::int(pc as i32), pc);
            return Ok(self.trace(pc, 0, None));
        }

        let insn = self.instruction_memory[pc as usize];
        let instruction = match Instruction::decode(insn) {
            Ok(instruction) => instruction,
            Err(_) => {
                self.pc = self.pc.wrapping_add(1);
                self.trap(BAD_OP_TRAP, Word::int(insn as i32), pc);
                return Ok(self.trace(pc, insn, None));
            }
        };

        self.pc = self.pc.wrapping_add(1);
        let result = if instruction.eff() != 0 && self.emr & (1_u16 << instruction.eff()) == 0 {
            Err(Fault::new(EFFECT_TRAP, Word::int(instruction.eff() as i32)))
        } else {
            self.execute(instruction, pc)
        };

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
        let mut steps = 0_u64;
        while self.status.is_running() && steps < max_steps {
            traces.push(self.step()?);
            steps += 1;
        }
        Ok(RunReport {
            status: self.status,
            steps,
            step_limit_reached: self.status.is_running() && steps == max_steps,
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

    pub fn memory_word(&self, address: u32) -> Option<Word> {
        if let Some(word) = self.data_memory.get(address as usize) {
            return Some(*word);
        }
        let offset = address.checked_sub(MMIO_BASE)? as usize;
        if offset >= MMIO_WORDS {
            return None;
        }
        Some(self.mmio_word(offset))
    }

    pub fn uart_output(&self) -> &[u8] {
        &self.uart_output
    }

    fn execute(&mut self, instruction: Instruction, pc: u32) -> Result<Option<(u32, Word)>, Fault> {
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
                let tag = Tag::try_from((instruction.imm14_bits() & 0x0f) as u8).map_err(|_| {
                    Fault::new(TAG_TRAP, Word::int(instruction.imm14_bits() as i32))
                })?;
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
            Opcode::Sar => {
                self.binary_shift(instruction, |left, right| ((left as i32) >> right) as u32)
            }
            Opcode::AddI => self.immediate_int(instruction, |left, immediate| {
                left.checked_add(immediate).ok_or(OVERFLOW_TRAP)
            }),
            Opcode::AndI => {
                self.immediate_payload(instruction, |left, immediate| left & immediate as u32)
            }
            Opcode::OrI => {
                self.immediate_payload(instruction, |left, immediate| left | immediate as u32)
            }
            Opcode::XorI => {
                self.immediate_payload(instruction, |left, immediate| left ^ immediate as u32)
            }
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
                self.write_reg(instruction.rd(), Word::bool(value.tag() as u8 == tag));
                Ok(None)
            }
            Opcode::Load => self.load(instruction),
            Opcode::Store => self.store(instruction),
            Opcode::Alloc => self.allocate(instruction),
            Opcode::Hdr => self.header(instruction),
            Opcode::CDerive => self.derive_capability(instruction),
            Opcode::CAndPerm => self.and_capability_permissions(instruction),
            Opcode::CDrop => {
                self.write_cap(instruction.cd(), Capability::null());
                Ok(None)
            }
            Opcode::CSeal => self.seal(instruction),
            Opcode::CUnseal => self.unseal(instruction),
            Opcode::CMov => {
                let value = self.caps[instruction.cs() as usize];
                self.write_cap(instruction.cd(), value);
                Ok(None)
            }
            Opcode::CInc => self.increment_capability(instruction),
            Opcode::CGet => self.get_capability_field(instruction),
            Opcode::CSpecial => self.get_special_capability(instruction),
            Opcode::Branch => self.relative_jump(pc, instruction.imm22()),
            Opcode::BranchTrue => {
                if self.read_bool(instruction.rd())? {
                    self.relative_jump(pc, instruction.imm18())
                } else {
                    Ok(None)
                }
            }
            Opcode::BranchFalse => {
                if !self.read_bool(instruction.rd())? {
                    self.relative_jump(pc, instruction.imm18())
                } else {
                    Ok(None)
                }
            }
            Opcode::Jump => {
                let value = self.read_reg(instruction.rd())?;
                if value.tag() != Tag::Code {
                    return Err(Fault::new(TAG_TRAP, value));
                }
                self.set_code_target(value.payload())
            }
            Opcode::Call => {
                self.push_call(self.pc, self.pcc)?;
                self.relative_jump(pc, instruction.imm22())
            }
            Opcode::CallC => self.call_capability(instruction),
            Opcode::CallCL => self.call_closure(instruction),
            Opcode::Ret => self.return_from_call(),
            Opcode::Withe => self.with_effects(instruction),
            Opcode::EndE => self.end_effects(),
            Opcode::GetE => {
                self.write_reg(instruction.rd(), Word::int(self.emr as i32));
                Ok(None)
            }
            Opcode::ReqE => {
                let required = (instruction.imm18_bits() & 0xffff) as u16;
                if self.emr & required != required {
                    Err(Fault::new(EFFECT_TRAP, Word::int(required as i32)))
                } else {
                    Ok(None)
                }
            }
            Opcode::Trap => {
                let cause = 0x80 | (instruction.imm22_bits() as u8 & 0x7f);
                Err(Fault::new(cause, Word::int(cause as i32)))
            }
            Opcode::TRet => {
                self.status = MachineStatus::Halted { exit_code: 0 };
                Ok(None)
            }
        }
    }

    fn binary_int<F>(
        &mut self,
        instruction: Instruction,
        operation: F,
    ) -> Result<Option<(u32, Word)>, Fault>
    where
        F: FnOnce(i32, i32) -> Result<i32, u8>,
    {
        let left = self.read_int(instruction.rs1())?;
        let right = self.read_int(instruction.rs2())?;
        let result = operation(left, right).map_err(|cause| Fault::new(cause, Word::int(left)))?;
        self.write_reg(instruction.rd(), Word::int(result));
        Ok(None)
    }

    fn binary_payload<F>(
        &mut self,
        instruction: Instruction,
        operation: F,
    ) -> Result<Option<(u32, Word)>, Fault>
    where
        F: FnOnce(u32, u32) -> u32,
    {
        let left = self.read_int(instruction.rs1())? as u32;
        let right = self.read_int(instruction.rs2())? as u32;
        self.write_reg(instruction.rd(), Word::int(operation(left, right) as i32));
        Ok(None)
    }

    fn binary_shift<F>(
        &mut self,
        instruction: Instruction,
        operation: F,
    ) -> Result<Option<(u32, Word)>, Fault>
    where
        F: FnOnce(u32, u32) -> u32,
    {
        let left = self.read_int(instruction.rs1())? as u32;
        let right = self.read_int(instruction.rs2())?;
        let shift =
            u32::try_from(right).map_err(|_| Fault::new(OVERFLOW_TRAP, Word::int(right)))?;
        if shift >= 32 {
            return Err(Fault::new(OVERFLOW_TRAP, Word::int(right as i32)));
        }
        self.write_reg(instruction.rd(), Word::int(operation(left, shift) as i32));
        Ok(None)
    }

    fn immediate_int<F>(
        &mut self,
        instruction: Instruction,
        operation: F,
    ) -> Result<Option<(u32, Word)>, Fault>
    where
        F: FnOnce(i32, i32) -> Result<i32, u8>,
    {
        let left = self.read_int(instruction.rs1())?;
        let result = operation(left, instruction.imm14())
            .map_err(|cause| Fault::new(cause, Word::int(left)))?;
        self.write_reg(instruction.rd(), Word::int(result));
        Ok(None)
    }

    fn immediate_payload<F>(
        &mut self,
        instruction: Instruction,
        operation: F,
    ) -> Result<Option<(u32, Word)>, Fault>
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

    fn immediate_shift<F>(
        &mut self,
        instruction: Instruction,
        operation: F,
    ) -> Result<Option<(u32, Word)>, Fault>
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

    fn binary_bool<F>(
        &mut self,
        instruction: Instruction,
        operation: F,
    ) -> Result<Option<(u32, Word)>, Fault>
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
            2..=5 if matches!(left.tag(), Tag::Int | Tag::Char) => {
                let left = left.payload();
                let right = right.payload();
                match condition {
                    2 => (left as i32) < (right as i32),
                    3 => (left as i32) <= (right as i32),
                    4 => (left as i32) > (right as i32),
                    _ => (left as i32) >= (right as i32),
                }
            }
            6 | 7 if matches!(left.tag(), Tag::Int | Tag::Char) => match condition {
                6 => left.payload() < right.payload(),
                _ => left.payload() <= right.payload(),
            },
            _ => return Err(Fault::new(TAG_TRAP, left)),
        };
        self.write_reg(instruction.rd(), Word::bool(result));
        Ok(None)
    }

    fn load(&mut self, instruction: Instruction) -> Result<Option<(u32, Word)>, Fault> {
        let address = self.resolve_memory(instruction, Permissions::READ)?;
        let value = self
            .read_memory(address)
            .ok_or_else(|| Fault::new(CAP_BOUND_TRAP, Word::int(address as i32)))?;
        if value.tag() == Tag::Poison {
            return Err(Fault::new(POISON_TRAP, value));
        }
        self.write_reg(instruction.rd(), value);
        Ok(None)
    }

    fn store(&mut self, instruction: Instruction) -> Result<Option<(u32, Word)>, Fault> {
        let value = self.read_reg(instruction.rd())?;
        let address = self.resolve_memory(instruction, Permissions::WRITE)?;
        if !self.write_memory(address, value) {
            return Err(Fault::new(CAP_BOUND_TRAP, Word::int(address as i32)));
        }
        Ok(Some((address, value)))
    }

    fn resolve_memory(
        &self,
        instruction: Instruction,
        permission: Permissions,
    ) -> Result<u32, Fault> {
        let capability_index = instruction.cap();
        let capability = self.caps[capability_index as usize];
        if !capability.permissions().contains(permission) {
            return Err(Fault::new(
                CAP_PERM_TRAP,
                Word::int(capability_index as i32),
            ));
        }
        let base = i64::from(self.read_int(instruction.base_reg())?);
        let relative = base + i64::from(instruction.imm10());
        let address = i64::from(capability.base()) + relative;
        let tval = if (0..=i64::from(u32::MAX)).contains(&address) {
            Word::int(address as i32)
        } else {
            Word::int(capability.base() as i32)
        };
        if relative < 0 || relative >= i64::from(capability.len()) {
            return Err(Fault::new(CAP_BOUND_TRAP, tval));
        }
        if !(0..=i64::from(u32::MAX)).contains(&address) {
            return Err(Fault::new(CAP_BOUND_TRAP, tval));
        }
        let address = address as u32;
        let is_mmio = (MMIO_BASE..MMIO_BASE + MMIO_WORDS as u32).contains(&address);
        if is_mmio {
            if !capability.permissions().contains(Permissions::MMIO) {
                return Err(Fault::new(
                    CAP_PERM_TRAP,
                    Word::int(capability_index as i32),
                ));
            }
            if instruction.eff() != 1 {
                return Err(Fault::new(EFFECT_TRAP, Word::int(instruction.eff() as i32)));
            }
        } else if address as usize >= DATA_WORDS {
            return Err(Fault::new(CAP_BOUND_TRAP, Word::int(address as i32)));
        }
        Ok(address)
    }

    fn allocate(&mut self, instruction: Instruction) -> Result<Option<(u32, Word)>, Fault> {
        if instruction.eff() != 4 {
            return Err(Fault::new(BAD_OP_TRAP, Word::int(instruction.eff() as i32)));
        }
        if !self.hpc.permissions().contains(Permissions::ALLOCATE) {
            return Err(Fault::new(CAP_PERM_TRAP, Word::int(0)));
        }
        let size = self.read_int(instruction.rs1())?;
        if size < 0 {
            return Err(Fault::new(HEAP_TRAP, Word::int(size)));
        }
        let size = size as u32;
        let type_id = u32::from(instruction.imm14_bits() & 0x0fff);
        let old_hp = self.hp;
        let end = match old_hp
            .checked_add(size)
            .and_then(|value| value.checked_add(1))
        {
            Some(end) => end,
            None => return Err(Fault::new(HEAP_TRAP, Word::int(size as i32))),
        };
        let heap_end = u64::from(self.hpc.base()) + u64::from(self.hpc.len());
        if u64::from(end) > heap_end || end as usize > DATA_WORDS {
            return Err(Fault::new(HEAP_TRAP, Word::int(size as i32)));
        }
        let header = Word::try_new(Tag::Header, ((size & 0x000f_ffff) << 12) | type_id)
            .map_err(|_| Fault::new(TAG_TRAP, Word::int(type_id as i32)))?;
        self.data_memory[old_hp as usize] = header;
        for address in old_hp + 1..end {
            self.data_memory[address as usize] = Word::poison();
        }
        self.hp = end;
        let pointer = Word::try_new(Tag::Ptr, old_hp + 1)
            .map_err(|_| Fault::new(HEAP_TRAP, Word::int(old_hp as i32)))?;
        self.write_reg(instruction.rd(), pointer);
        Ok(Some((old_hp, header)))
    }

    fn header(&mut self, instruction: Instruction) -> Result<Option<(u32, Word)>, Fault> {
        let pointer = self.read_reg(instruction.rs1())?;
        if pointer.tag() != Tag::Ptr || pointer.payload() == 0 {
            return Err(Fault::new(TAG_TRAP, pointer));
        }
        let address = pointer.payload() - 1;
        let header = self
            .read_memory(address)
            .ok_or_else(|| Fault::new(TAG_TRAP, pointer))?;
        if header.tag() != Tag::Header {
            return Err(Fault::new(TAG_TRAP, header));
        }
        self.write_reg(instruction.rd(), header);
        Ok(None)
    }

    fn derive_capability(
        &mut self,
        instruction: Instruction,
    ) -> Result<Option<(u32, Word)>, Fault> {
        let parent = self.caps[instruction.cs() as usize];
        self.require_derive(parent)?;
        let offset = self.nonnegative_int(instruction.base_reg())?;
        let len = self.nonnegative_int(instruction.aux_reg())?;
        let child = parent
            .derive(offset, len)
            .map_err(|_| Fault::new(CAP_BOUND_TRAP, Word::int(offset as i32)))?;
        self.write_cap(instruction.cd(), child);
        Ok(None)
    }

    fn and_capability_permissions(
        &mut self,
        instruction: Instruction,
    ) -> Result<Option<(u32, Word)>, Fault> {
        let parent = self.caps[instruction.cs() as usize];
        self.require_derive(parent)?;
        let permissions = Permissions::from_bits((instruction.aux10_bits() & 0xff) as u8);
        self.write_cap(instruction.cd(), parent.attenuate(permissions));
        Ok(None)
    }

    fn increment_capability(
        &mut self,
        instruction: Instruction,
    ) -> Result<Option<(u32, Word)>, Fault> {
        let parent = self.caps[instruction.cs() as usize];
        self.require_derive(parent)?;
        let offset = self.nonnegative_int(instruction.base_reg())?;
        let len = parent
            .len()
            .checked_sub(offset)
            .ok_or_else(|| Fault::new(CAP_BOUND_TRAP, Word::int(offset as i32)))?;
        let child = parent
            .derive(offset, len)
            .map_err(|_| Fault::new(CAP_BOUND_TRAP, Word::int(offset as i32)))?;
        self.write_cap(instruction.cd(), child);
        Ok(None)
    }

    fn get_capability_field(
        &mut self,
        instruction: Instruction,
    ) -> Result<Option<(u32, Word)>, Fault> {
        let capability = self.caps[instruction.cs() as usize];
        let value = match instruction.aux10_bits() & 0x07 {
            0 => Word::int(capability.base() as i32),
            1 => Word::int(capability.len() as i32),
            2 => Word::int(capability.permissions().bits() as i32),
            3 => Word::int(capability.otype() as i32),
            4 => Word::bool(capability.is_null()),
            _ => {
                return Err(Fault::new(
                    BAD_OP_TRAP,
                    Word::int(instruction.aux10_bits() as i32),
                ))
            }
        };
        self.write_reg(instruction.rd(), value);
        Ok(None)
    }

    fn get_special_capability(
        &mut self,
        instruction: Instruction,
    ) -> Result<Option<(u32, Word)>, Fault> {
        let capability = match instruction.aux10_bits() & 0x03 {
            0 => self.pcc,
            1 => self.hpc,
            _ => {
                return Err(Fault::new(
                    BAD_OP_TRAP,
                    Word::int(instruction.aux10_bits() as i32),
                ))
            }
        };
        self.write_cap(instruction.cd(), capability);
        Ok(None)
    }

    fn seal(&mut self, instruction: Instruction) -> Result<Option<(u32, Word)>, Fault> {
        if instruction.eff() != 10 {
            return Err(Fault::new(BAD_OP_TRAP, Word::int(instruction.eff() as i32)));
        }
        let key = self.caps[(instruction.rs2() & 0x07) as usize];
        if !key.permissions().contains(Permissions::SEAL) {
            return Err(Fault::new(
                CAP_PERM_TRAP,
                Word::int(instruction.rs2() as i32),
            ));
        }
        let value = self.read_reg(instruction.rs1())?;
        let token = self.allocate_seal_token()?;
        self.sealed_words.push(SealedWord {
            token,
            otype: key.otype(),
            value,
        });
        let sealed = Word::try_new(Tag::Sealed, (u32::from(key.otype()) << 24) | token)
            .map_err(|_| Fault::new(SEAL_TRAP, value))?;
        self.write_reg(instruction.rd(), sealed);
        Ok(None)
    }

    fn unseal(&mut self, instruction: Instruction) -> Result<Option<(u32, Word)>, Fault> {
        if instruction.eff() != 10 {
            return Err(Fault::new(BAD_OP_TRAP, Word::int(instruction.eff() as i32)));
        }
        let key = self.caps[(instruction.rs2() & 0x07) as usize];
        if !key.permissions().contains(Permissions::SEAL) {
            return Err(Fault::new(
                CAP_PERM_TRAP,
                Word::int(instruction.rs2() as i32),
            ));
        }
        let sealed = self.read_reg(instruction.rs1())?;
        if sealed.tag() != Tag::Sealed {
            return Err(Fault::new(SEAL_TRAP, sealed));
        }
        let token = sealed.payload() & 0x00ff_ffff;
        let otype = (sealed.payload() >> 24) as u8;
        let value = self
            .sealed_words
            .iter()
            .find(|entry| {
                entry.token == token && entry.otype == otype && entry.otype == key.otype()
            })
            .map(|entry| entry.value)
            .ok_or_else(|| Fault::new(SEAL_TRAP, sealed))?;
        self.write_reg(instruction.rd(), value);
        Ok(None)
    }

    fn allocate_seal_token(&mut self) -> Result<u32, Fault> {
        for _ in 0..0x00ff_ffff {
            let token = self.next_seal_token & 0x00ff_ffff;
            self.next_seal_token = (token + 1) & 0x00ff_ffff;
            if token != 0 && !self.sealed_words.iter().any(|entry| entry.token == token) {
                return Ok(token);
            }
        }
        Err(Fault::new(SEAL_TRAP, Word::unit()))
    }

    fn call_capability(&mut self, instruction: Instruction) -> Result<Option<(u32, Word)>, Fault> {
        let capability = self.caps[instruction.cd() as usize];
        if !capability.permissions().contains(Permissions::EXECUTE) {
            return Err(Fault::new(
                CAP_PERM_TRAP,
                Word::int(instruction.cd() as i32),
            ));
        }
        let offset = self.nonnegative_int(instruction.base_reg())?;
        if offset >= capability.len() {
            return Err(Fault::new(CAP_BOUND_TRAP, Word::int(offset as i32)));
        }
        let target = u64::from(capability.base()) + u64::from(offset);
        if target > u64::from(u32::MAX) {
            return Err(Fault::new(CAP_BOUND_TRAP, Word::int(offset as i32)));
        }
        self.push_call(self.pc, self.pcc)?;
        self.pcc = capability;
        self.set_code_target(target as u32)
    }

    fn call_closure(&mut self, instruction: Instruction) -> Result<Option<(u32, Word)>, Fault> {
        let pointer = self.read_reg(instruction.rd())?;
        if pointer.tag() != Tag::Ptr || pointer.payload() == 0 {
            return Err(Fault::new(TAG_TRAP, pointer));
        }
        let header_address = pointer.payload() - 1;
        let header = self
            .read_memory(header_address)
            .ok_or_else(|| Fault::new(TAG_TRAP, pointer))?;
        if header.tag() != Tag::Header || header.payload() & 0x0fff != 1 {
            return Err(Fault::new(SEAL_TRAP, header));
        }
        let code = self
            .read_memory(pointer.payload())
            .ok_or_else(|| Fault::new(TAG_TRAP, pointer))?;
        let environment = self
            .read_memory(
                pointer.payload().checked_add(1).ok_or_else(|| {
                    Fault::new(CAP_BOUND_TRAP, Word::int(pointer.payload() as i32))
                })?,
            )
            .ok_or_else(|| Fault::new(TAG_TRAP, pointer))?;
        if code.tag() != Tag::Code {
            return Err(Fault::new(TAG_TRAP, code));
        }
        self.ensure_code_target(code.payload())?;
        self.push_call(self.pc, self.pcc)?;
        self.write_reg(14, environment);
        self.set_code_target(code.payload())
    }

    fn return_from_call(&mut self) -> Result<Option<(u32, Word)>, Fault> {
        match self.frames.last().copied() {
            Some(Frame::Call {
                return_pc,
                emr,
                pcc,
            }) => {
                self.frames.pop();
                self.sync_fsp();
                self.pc = return_pc;
                self.emr = emr;
                self.pcc = pcc;
                Ok(None)
            }
            _ => Err(Fault::new(FRAME_TRAP, Word::unit())),
        }
    }

    fn with_effects(&mut self, instruction: Instruction) -> Result<Option<(u32, Word)>, Fault> {
        self.push_effect(self.emr)?;
        self.emr = (self.emr & (instruction.imm18_bits() & 0xffff) as u16) as u16;
        Ok(None)
    }

    fn end_effects(&mut self) -> Result<Option<(u32, Word)>, Fault> {
        match self.frames.last().copied() {
            Some(Frame::Effect { emr }) => {
                self.frames.pop();
                self.sync_fsp();
                self.emr = emr;
                Ok(None)
            }
            _ => Err(Fault::new(FRAME_TRAP, Word::unit())),
        }
    }

    fn push_call(&mut self, return_pc: u32, pcc: Capability) -> Result<(), Fault> {
        if self.frames.len() >= FRAME_LIMIT {
            return Err(Fault::new(FRAME_TRAP, Word::unit()));
        }
        self.frames.push(Frame::Call {
            return_pc,
            emr: self.emr,
            pcc,
        });
        self.sync_fsp();
        Ok(())
    }

    fn push_effect(&mut self, emr: u16) -> Result<(), Fault> {
        if self.frames.len() >= FRAME_LIMIT {
            return Err(Fault::new(FRAME_TRAP, Word::unit()));
        }
        self.frames.push(Frame::Effect { emr });
        self.sync_fsp();
        Ok(())
    }

    fn sync_fsp(&mut self) {
        self.fsp = self.frames.len() as u16;
    }

    fn relative_jump(&mut self, base: u32, offset: i32) -> Result<Option<(u32, Word)>, Fault> {
        let target = i64::from(base) + i64::from(offset);
        if !(0..=i64::from(u32::MAX)).contains(&target) {
            return Err(Fault::new(BAD_PC_TRAP, Word::int(base as i32)));
        }
        self.set_code_target(target as u32)
    }

    fn set_code_target(&mut self, target: u32) -> Result<Option<(u32, Word)>, Fault> {
        if !self.code_address_valid(target, self.pcc) {
            return Err(Fault::new(BAD_PC_TRAP, Word::int(target as i32)));
        }
        self.pc = target;
        Ok(None)
    }

    fn ensure_code_target(&self, target: u32) -> Result<(), Fault> {
        if self.code_address_valid(target, self.pcc) {
            Ok(())
        } else {
            Err(Fault::new(BAD_PC_TRAP, Word::int(target as i32)))
        }
    }

    fn code_address_valid(&self, address: u32, capability: Capability) -> bool {
        capability.permissions().contains(Permissions::EXECUTE)
            && u64::from(address) >= u64::from(capability.base())
            && u64::from(address) < u64::from(capability.base()) + u64::from(capability.len())
            && (address as usize) < INSTRUCTION_WORDS
    }

    fn require_derive(&self, capability: Capability) -> Result<(), Fault> {
        if capability.permissions().contains(Permissions::DERIVE) {
            Ok(())
        } else {
            Err(Fault::new(
                CAP_PERM_TRAP,
                Word::int(capability.otype() as i32),
            ))
        }
    }

    fn nonnegative_int(&self, index: u8) -> Result<u32, Fault> {
        let value = self.read_int(index)?;
        u32::try_from(value).map_err(|_| Fault::new(CAP_BOUND_TRAP, Word::int(value)))
    }

    fn read_reg(&self, index: u8) -> Result<Word, Fault> {
        let value = self.regs[index as usize];
        if value.tag() == Tag::Poison {
            Err(Fault::new(POISON_TRAP, value))
        } else {
            Ok(value)
        }
    }

    fn read_int(&self, index: u8) -> Result<i32, Fault> {
        let value = self.read_reg(index)?;
        value.as_i32().map_err(|_| Fault::new(TAG_TRAP, value))
    }

    fn read_bool(&self, index: u8) -> Result<bool, Fault> {
        let value = self.read_reg(index)?;
        if value.tag() != Tag::Bool {
            return Err(Fault::new(TAG_TRAP, value));
        }
        Ok(value.payload() != 0)
    }

    fn write_reg(&mut self, index: u8, value: Word) {
        if index != 0 {
            self.regs[index as usize] = value;
        }
    }

    fn write_cap(&mut self, index: u8, value: Capability) {
        if index != 0 {
            self.caps[index as usize] = value;
        }
    }

    fn read_memory(&mut self, address: u32) -> Option<Word> {
        if let Some(word) = self.data_memory.get(address as usize) {
            return Some(*word);
        }
        let offset = address.checked_sub(MMIO_BASE)? as usize;
        if offset >= MMIO_WORDS {
            return None;
        }
        if offset == RNG {
            self.rng_state = self
                .rng_state
                .wrapping_mul(1_664_525)
                .wrapping_add(1_013_904_223);
            return Some(Word::int(self.rng_state as i32));
        }
        Some(self.mmio_word(offset))
    }

    fn write_memory(&mut self, address: u32, value: Word) -> bool {
        if let Some(word) = self.data_memory.get_mut(address as usize) {
            *word = value;
            return true;
        }
        let Some(offset) = address.checked_sub(MMIO_BASE).map(|value| value as usize) else {
            return false;
        };
        if offset >= MMIO_WORDS {
            return false;
        }
        if offset == UART_TX {
            self.uart_output.push(value.payload() as u8);
        }
        self.mmio[offset] = value;
        true
    }

    fn mmio_word(&self, offset: usize) -> Word {
        match offset {
            CYCLE_LO => Word::int(self.cycles as u32 as i32),
            CYCLE_HI => Word::int((self.cycles >> 32) as u32 as i32),
            RNG => Word::int(self.rng_state as i32),
            _ => self.mmio[offset],
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
    BadPc(u32),
    Stopped,
}

impl fmt::Display for MachineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProgramTooLarge(words) => {
                write!(formatter, "program has {words} words; maximum is 65536")
            }
            Self::Capability(error) => write!(formatter, "invalid boot capability: {error}"),
            Self::BadPc(pc) => write!(
                formatter,
                "instruction fetch outside memory at PC 0x{pc:08x}"
            ),
            Self::Stopped => write!(formatter, "machine has already halted or trapped"),
        }
    }
}

impl std::error::Error for MachineError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
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

pub const fn trap_name(cause: u8) -> &'static str {
    match cause {
        TAG_TRAP => "TAG",
        POISON_TRAP => "POISON",
        OVERFLOW_TRAP => "OVF",
        DIV_ZERO_TRAP => "DIV0",
        CAP_BOUND_TRAP => "CAPBOUND",
        CAP_PERM_TRAP => "CAPPERM",
        EFFECT_TRAP => "EFFECT",
        HEAP_TRAP => "HEAP",
        FRAME_TRAP => "FRAME",
        SEAL_TRAP => "SEAL",
        BAD_OP_TRAP => "BADOP",
        BAD_PC_TRAP => "BADPC",
        value if value >= 0x80 => "USER",
        _ => "UNKNOWN",
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
