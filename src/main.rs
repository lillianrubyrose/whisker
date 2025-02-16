mod csr;
mod gdb;
mod insn;
mod mem;
mod ty;

#[cfg(not(target_pointer_width = "64"))]
compile_error!("whisker only supports 64bit architectures");

use std::collections::HashSet;
use std::fs;
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not};
use std::path::PathBuf;

use clap::{command, Parser, Subcommand};

use csr::ControlStatusRegisters;
use gdb::WhiskerEventLoop;
use gdbstub::conn::ConnectionExt;
use gdbstub::stub::GdbStub;
use insn::{Instruction, IntInstruction};
use mem::Memory;
use ty::RegisterIndex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupportedExtensions(u64);

impl SupportedExtensions {
	pub const ATOMIC: Self = Self(1 << 0);
	pub const B: Self = Self(1 << 1);
	pub const COMPRESSED: Self = Self(1 << 2);
	pub const DOUBLE: Self = Self(1 << 3);
	pub const E: Self = Self(1 << 4);
	pub const FLOAT: Self = Self(1 << 5);
	pub const G_RESERVED: Self = Self(1 << 6);
	pub const HYPERVISOR: Self = Self(1 << 7);
	pub const INTEGER: Self = Self(1 << 8);
	pub const J_RESERVED: Self = Self(1 << 9);
	pub const K_RESERVED: Self = Self(1 << 10);
	pub const L_RESERVED: Self = Self(1 << 11);
	pub const MULTIPLY: Self = Self(1 << 12);
	pub const N_RESERVED: Self = Self(1 << 13);
	pub const O_RESERVED: Self = Self(1 << 14);
	pub const P_RESERVED: Self = Self(1 << 15);
	pub const QUAD_FLOAT: Self = Self(1 << 16);
	pub const R_RESERVED: Self = Self(1 << 17);
	pub const SUPERVISOR: Self = Self(1 << 18);
	pub const T_RESERVED: Self = Self(1 << 19);
	pub const USER_MODE: Self = Self(1 << 20);
	pub const VECTOR: Self = Self(1 << 21);
	pub const W_RESERVED: Self = Self(1 << 22);
	pub const NON_STANDARD: Self = Self(1 << 23);
	pub const Y_RESERVED: Self = Self(1 << 24);
	pub const Z_RESERVED: Self = Self(1 << 25);

	pub const fn empty() -> Self {
		SupportedExtensions(0)
	}

	pub const fn has(self, other: Self) -> bool {
		(self.0 & other.0) == other.0
	}

	pub fn insert(&mut self, other: Self) -> &mut Self {
		self.0 |= other.0;
		self
	}

	pub fn remove(&mut self, other: Self) -> &mut Self {
		self.0 &= !other.0;
		self
	}
}

impl BitOr for SupportedExtensions {
	type Output = Self;
	fn bitor(self, rhs: Self) -> Self::Output {
		SupportedExtensions(self.0 | rhs.0)
	}
}

impl BitOrAssign for SupportedExtensions {
	fn bitor_assign(&mut self, rhs: Self) {
		self.0 |= rhs.0;
	}
}

impl BitAnd for SupportedExtensions {
	type Output = Self;
	fn bitand(self, rhs: Self) -> Self::Output {
		SupportedExtensions(self.0 & rhs.0)
	}
}

impl BitAndAssign for SupportedExtensions {
	fn bitand_assign(&mut self, rhs: Self) {
		self.0 &= rhs.0;
	}
}

impl Not for SupportedExtensions {
	type Output = Self;
	fn not(self) -> Self::Output {
		SupportedExtensions(!self.0)
	}
}

impl Default for SupportedExtensions {
	fn default() -> Self {
		Self::INTEGER
	}
}

#[derive(Default, Debug)]
pub struct Registers {
	x: [u64; 31],

	pub pc: u64,
}

impl Registers {
	pub fn get(&self, index: RegisterIndex) -> u64 {
		let index = usize::from(index.inner());
		if index == 0 {
			0
		} else {
			self.x[index - 1]
		}
	}

	pub fn set(&mut self, index: RegisterIndex, value: u64) {
		let index = usize::from(index.inner());
		if index == 0 {
			// writes to r0 are ignored
		} else {
			self.x[index - 1] = value;
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WhiskerExecState {
	Step,
	Running,
	Paused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WhiskerExecErr {
	Stepped,
	HitBreakpoint,
	Paused,
}

#[derive(Debug)]
pub struct WhiskerCpu {
	pub supported_extensions: SupportedExtensions,
	pub mem: Memory,
	pub registers: Registers,

	should_trap: bool,

	pub csrs: ControlStatusRegisters,

	pub cycles: u64,
	pub exec_state: WhiskerExecState,

	pub breakpoints: HashSet<u64>,
}

impl WhiskerCpu {
	pub fn new(supported_extensions: SupportedExtensions, mem: Memory) -> Self {
		Self {
			supported_extensions,
			mem,
			registers: Registers::default(),

			should_trap: false,
			csrs: ControlStatusRegisters::new(),

			cycles: 0,
			exec_state: WhiskerExecState::Paused,
			breakpoints: HashSet::default(),
		}
	}

	fn execute_i_insn(&mut self, insn: IntInstruction, start_pc: u64) {
		match insn {
			IntInstruction::LoadUpperImmediate { dst, val } => {
				self.registers.set(dst, val as u64);
			}
			IntInstruction::AddUpperImmediateToPc { dst, val } => {
				self.registers.set(dst, self.registers.pc.wrapping_add_signed(val));
			}
			IntInstruction::StoreByte { dst, dst_offset, src } => {
				let offset = self.registers.get(dst).wrapping_add_signed(dst_offset);
				let val = self.registers.get(src) as u8;
				self.mem.write_u8(offset, val);
			}
			IntInstruction::StoreHalf { dst, dst_offset, src } => {
				let offset = self.registers.get(dst).wrapping_add_signed(dst_offset);
				let val = self.registers.get(src) as u16;
				self.mem.write_u16(offset, val);
			}
			IntInstruction::StoreWord { dst, dst_offset, src } => {
				let offset = self.registers.get(dst).wrapping_add_signed(dst_offset);
				let val = self.registers.get(src) as u32;
				self.mem.write_u32(offset, val);
			}
			IntInstruction::StoreDoubleWord { dst, dst_offset, src } => {
				let offset = self.registers.get(dst).wrapping_add_signed(dst_offset);
				let val = self.registers.get(src);
				self.mem.write_u64(offset, val);
			}
			IntInstruction::LoadByte { dst, src, src_offset } => {
				let offset = self.registers.get(src).wrapping_add_signed(src_offset);
				let val = self.mem.read_u8(offset) as u64;

				let reg_val = self.registers.get(dst);
				let val = (reg_val & 0xFFFFFFFF_FFFFFF00) | val;
				self.registers.set(dst, val);
			}
			IntInstruction::LoadHalf { dst, src, src_offset } => {
				let offset = self.registers.get(src).wrapping_add_signed(src_offset);
				let val = self.mem.read_u16(offset) as u64;

				let reg_val = self.registers.get(dst);
				let val = (reg_val & 0xFFFFFFFF_FFFF0000) | val;
				self.registers.set(dst, val);
			}
			IntInstruction::LoadWord { dst, src, src_offset } => {
				let offset = self.registers.get(src).wrapping_add_signed(src_offset);
				let val = self.mem.read_u32(offset) as u64;

				let reg_val = self.registers.get(dst);
				let val = (reg_val & 0xFFFFFFFF_00000000) | val;
				self.registers.set(dst, val);
			}
			IntInstruction::LoadDoubleWord { dst, src, src_offset } => {
				let offset = self.registers.get(src).wrapping_add_signed(src_offset);
				let val = self.mem.read_u64(offset);
				self.registers.set(dst, val);
			}
			IntInstruction::LoadByteZeroExtend { dst, src, src_offset } => {
				let offset = self.registers.get(src).wrapping_add_signed(src_offset);
				let val = self.mem.read_u8(offset) as u64;
				self.registers.set(dst, val);
			}
			IntInstruction::LoadHalfZeroExtend { dst, src, src_offset } => {
				let offset = self.registers.get(src).wrapping_add_signed(src_offset);
				let val = self.mem.read_u16(offset) as u64;
				self.registers.set(dst, val);
			}
			IntInstruction::LoadWordZeroExtend { dst, src, src_offset } => {
				let offset = self.registers.get(src).wrapping_add_signed(src_offset);
				let val = self.mem.read_u32(offset) as u64;
				self.registers.set(dst, val);
			}
			IntInstruction::JumpAndLink { link_reg, jmp_off } => {
				self.registers.set(link_reg, start_pc + 4);
				self.registers.pc = start_pc.wrapping_add_signed(jmp_off);
			}
			IntInstruction::Add { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);
				self.registers.set(dst, lhs.wrapping_add(rhs));
			}
			IntInstruction::Sub { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);
				self.registers.set(dst, lhs.wrapping_sub(rhs));
			}
			IntInstruction::Xor { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);
				self.registers.set(dst, lhs ^ rhs);
			}
			IntInstruction::Or { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);
				self.registers.set(dst, lhs | rhs);
			}
			IntInstruction::And { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);
				self.registers.set(dst, lhs & rhs);
			}
			IntInstruction::ShiftLeftLogical { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);
				self.registers.set(dst, lhs.wrapping_shl(rhs as u32));
			}
			IntInstruction::ShiftRightLogical { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);
				self.registers.set(dst, lhs.wrapping_shr(rhs as u32));
			}
			IntInstruction::ShiftRightArithmetic { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs) as i64;
				let rhs = self.registers.get(rhs);
				self.registers.set(dst, lhs.wrapping_shr(rhs as u32) as u64);
			}
			IntInstruction::SetLessThan { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs) as i64;
				let rhs = self.registers.get(rhs) as i64;
				self.registers.set(dst, (lhs < rhs) as u64);
			}
			IntInstruction::SetLessThanUnsigned { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);
				self.registers.set(dst, (lhs < rhs) as u64);
			}
			IntInstruction::JumpAndLinkRegister {
				link_reg,
				jmp_reg,
				jmp_off,
			} => {
				self.registers.set(link_reg, start_pc + 4);
				self.registers.pc = self.registers.get(jmp_reg).wrapping_add_signed(jmp_off) & !1;
			}

			IntInstruction::AddImmediate { dst, lhs, rhs } => {
				self.registers
					.set(dst, self.registers.get(lhs).wrapping_add_signed(rhs));
			}
			IntInstruction::XorImmediate { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				self.registers.set(dst, lhs ^ (rhs as u64));
			}
			IntInstruction::OrImmediate { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				self.registers.set(dst, lhs | (rhs as u64));
			}
			IntInstruction::AndImmediate { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				self.registers.set(dst, lhs & (rhs as u64));
			}
			IntInstruction::ShiftLeftLogicalImmediate { dst, lhs, shift_amt } => {
				let lhs = self.registers.get(lhs);
				self.registers.set(dst, lhs.wrapping_shr(shift_amt));
			}
			IntInstruction::ShiftRightLogicalImmediate { dst, lhs, shift_amt } => {
				let lhs = self.registers.get(lhs);
				self.registers.set(dst, lhs.wrapping_shr(shift_amt));
			}
			IntInstruction::ShiftRightArithmeticImmediate { dst, lhs, shift_amt } => {
				let lhs = self.registers.get(lhs) as i64;
				self.registers.set(dst, lhs.wrapping_shr(shift_amt) as u64);
			}
			IntInstruction::SetLessThanImmediate { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs) as i64;
				self.registers.set(dst, (lhs < rhs) as u64);
			}
			IntInstruction::SetLessThanUnsignedImmediate { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				let rhs = rhs as u64;
				self.registers.set(dst, (lhs < rhs) as u64);
			}
			IntInstruction::AddImmediateWord { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs) as u32;
				self.registers.set(dst, lhs.wrapping_add_signed(rhs) as u64);
			}
			IntInstruction::ShiftLeftLogicalImmediateWord { dst, lhs, shift_amt } => {
				let lhs = self.registers.get(lhs) as u32;
				self.registers.set(dst, lhs.wrapping_shl(shift_amt) as u64);
			}
			IntInstruction::ShiftRightLogicalImmediateWord { dst, lhs, shift_amt } => {
				let lhs = self.registers.get(lhs) as u32;
				self.registers.set(dst, lhs.wrapping_shr(shift_amt) as u64);
			}
			IntInstruction::ShiftRightArithmeticImmediateWord { dst, lhs, shift_amt } => {
				let lhs = self.registers.get(lhs) as i32;
				self.registers.set(dst, lhs.wrapping_shr(shift_amt) as u64);
			}

			// ============
			// BRANCH
			// ============
			IntInstruction::BranchEqual { lhs, rhs, imm } => {
				if self.registers.get(lhs) == self.registers.get(rhs) {
					self.registers.pc = start_pc.wrapping_add_signed(imm);
				}
			}
			IntInstruction::BranchNotEqual { lhs, rhs, imm } => {
				if self.registers.get(lhs) != self.registers.get(rhs) {
					self.registers.pc = start_pc.wrapping_add_signed(imm);
				}
			}
			IntInstruction::BranchLessThan { lhs, rhs, imm } => {
				if (self.registers.get(lhs) as i64) < self.registers.get(rhs) as i64 {
					self.registers.pc = start_pc.wrapping_add_signed(imm);
				}
			}
			IntInstruction::BranchGreaterEqual { lhs, rhs, imm } => {
				if (self.registers.get(lhs) as i64) >= self.registers.get(rhs) as i64 {
					self.registers.pc = start_pc.wrapping_add_signed(imm);
				}
			}
			IntInstruction::BranchLessThanUnsigned { lhs, rhs, imm } => {
				if self.registers.get(lhs) < self.registers.get(rhs) {
					self.registers.pc = start_pc.wrapping_add_signed(imm);
				}
			}
			IntInstruction::BranchGreaterEqualUnsigned { lhs, rhs, imm } => {
				if self.registers.get(lhs) >= self.registers.get(rhs) {
					self.registers.pc = start_pc.wrapping_add_signed(imm);
				}
			}

			// =========
			// SYSTEM
			// =========
			IntInstruction::ECall => todo!("ECALL"),
			IntInstruction::EBreak => todo!("EBREAK"),
		}
	}

	pub fn execute_one(&mut self) -> Result<(), WhiskerExecErr> {
		if self.should_trap {
			todo!("impl trap")
		}

		// some instructions (particularly jumps) need the program counter at the start of the instruction
		let start_pc = self.registers.pc;

		if self.breakpoints.contains(&start_pc) {
			return Err(WhiskerExecErr::HitBreakpoint);
		}

		match Instruction::fetch_instruction(self) {
			Ok((inst, size)) => {
				match inst {
					Instruction::IntExtension(insn) => self.execute_i_insn(insn, start_pc),
				}
				self.cycles += 1;
				self.registers.pc = self.registers.pc.wrapping_add(size);
				Ok(())
			}
			Err(()) => {
				// error during instruction decoding, trap was requested
				Ok(())
			}
		}
	}

	fn should_poll(&self) -> bool {
		self.cycles % 1024 == 0
	}

	// If this routine returns [None] then there's incoming GDB data
	pub fn execute<F: FnMut() -> bool>(&mut self, mut poll_incoming_data: F) -> Result<(), WhiskerExecErr> {
		match self.exec_state {
			WhiskerExecState::Step => {
				self.execute_one()?;
				Err(WhiskerExecErr::Stepped)
			}
			WhiskerExecState::Running => loop {
				if self.should_poll() && poll_incoming_data() {
					return Ok(());
				}
				self.execute_one()?;
			},
			WhiskerExecState::Paused => Err(WhiskerExecErr::Paused),
		}
	}

	fn request_trap(&mut self, trap: u64) {
		// trap causes have the high bit set if they are an interrupt, or unset for exceptions
		self.csrs.write_mcause(trap);
		self.should_trap = true;
	}
}

impl Default for WhiskerCpu {
	fn default() -> Self {
		Self::new(SupportedExtensions::default(), Memory::new(0x40_0000))
	}
}

#[derive(Debug, Parser)]
#[command(version)]
struct CliArgs {
	#[arg(short = 'g', long)]
	gdb: bool,

	#[command(subcommand)]
	command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
	Run {
		#[arg()]
		bootrom: PathBuf,
		#[arg(long, default_value_t = 0)]
		bootrom_offset: u64,
	},
}

fn main() {
	env_logger::init();
	let cli = CliArgs::parse();

	match cli.command {
		Commands::Run {
			bootrom,
			bootrom_offset,
		} => {
			let mut cpu = WhiskerCpu::default();

			let prog =
				fs::read(&bootrom).unwrap_or_else(|_| panic!("could not read bootrom file {}", bootrom.display()));

			cpu.mem.write_slice(bootrom_offset, prog.as_slice());
			cpu.registers.pc = bootrom_offset;
			cpu.registers.set(RegisterIndex::new(2).expect("to be valid"), 0x7FFF);

			if cli.gdb {
				let conn: Box<dyn ConnectionExt<Error = std::io::Error>> =
					Box::new(gdb::wait_for_tcp().expect("listener to bind"));
				let gdb = GdbStub::new(conn);
				match gdb.run_blocking::<WhiskerEventLoop>(&mut cpu) {
					Ok(dc_reason) => match dc_reason {
						gdbstub::stub::DisconnectReason::TargetExited(result) => {
							println!("Target exited: {result}")
						}
						gdbstub::stub::DisconnectReason::TargetTerminated(signal) => {
							println!("Target terminated: {signal:?}");
						}
						gdbstub::stub::DisconnectReason::Disconnect => {
							cpu.exec_state = WhiskerExecState::Running;
							loop {
								cpu.execute_one();
							}
						}
						gdbstub::stub::DisconnectReason::Kill => println!("(GDB) Received kill command"),
					},
					Err(err) => {
						dbg!(&err);
						if err.is_target_error() {
							println!(
								"target encountered a fatal error: {:?}",
								err.into_target_error().unwrap()
							)
						} else if err.is_connection_error() {
							let (err, kind) = err.into_connection_error().unwrap();
							println!("connection error: {kind:?} - {err:?}")
						} else {
							println!("gdbstub encountered a fatal error: {err:?}")
						}
					}
				}
			} else {
				cpu.exec_state = WhiskerExecState::Running;
				loop {
					cpu.execute_one();
				}
			}
		}
	}
}
