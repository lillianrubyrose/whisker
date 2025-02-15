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

use gdb::WhiskerEventLoop;
use gdbstub::conn::ConnectionExt;
use gdbstub::stub::GdbStub;
use insn::{Instruction, IntInstruction};
use mem::Memory;
use ty::RegisterIndex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupportedExtensions(u64);

impl SupportedExtensions {
	pub const INTEGER: SupportedExtensions = SupportedExtensions(0b1);

	pub const fn empty() -> Self {
		SupportedExtensions(0)
	}

	pub const fn all() -> Self {
		SupportedExtensions(u64::MAX)
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

pub enum WhiskerExecState {
	Step,
	Running,
	Paused,
}

pub enum WhiskerExecResult {
	Stepped,
	HitBreakpoint,
	Paused,
}

pub struct WhiskerCpu {
	pub supported_extensions: SupportedExtensions,
	pub mem: Memory,
	pub registers: Registers,

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
		}
	}

	pub fn execute_one(&mut self) -> Option<WhiskerExecResult> {
		// some instructions (particularly jumps) need the program counter at the start of the instruction
		let start_pc = self.registers.pc;

		if self.breakpoints.contains(&start_pc) {
			return Some(WhiskerExecResult::HitBreakpoint);
		}

		// increments pc to past the end of the instruction
		let insn = Instruction::fetch_instruction(self);
		match insn {
			Instruction::IntExtension(insn) => self.execute_i_insn(insn, start_pc),
		}
		self.cycles += 1;
		None
	}

	fn should_poll(&self) -> bool {
		self.cycles % 1024 == 0
	}

	// If this routine returns [None] then there's incoming GDB data
	pub fn execute<F: FnMut() -> bool>(&mut self, mut poll_incoming_data: F) -> Option<WhiskerExecResult> {
		match self.exec_state {
			WhiskerExecState::Step => self.execute_one().or_else(|| Some(WhiskerExecResult::Stepped)),
			WhiskerExecState::Running => loop {
				if self.should_poll() && poll_incoming_data() {
					return None;
				}

				if let Some(res) = self.execute_one() {
					return Some(res);
				}
			},
			WhiskerExecState::Paused => Some(WhiskerExecResult::Paused),
		}
	}
}

impl Default for WhiskerCpu {
	fn default() -> Self {
		Self {
			supported_extensions: SupportedExtensions::all(),
			mem: Memory::new(0x10001000),
			exec_state: WhiskerExecState::Paused,
			cycles: 0,
			registers: Default::default(),
			breakpoints: HashSet::default(),
		}
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
