#![forbid(
	clippy::as_conversions,
	reason = "confusion with sign extension and zero extension has repeatedly caused bugs.
	use `truncate`, `extend`, `cast_signed`, `cast_unsigned`, `sign_extend`, and `zero_extend` instead."
)]

use std::{assert_matches::assert_matches, cmp::Ordering, collections::BTreeMap, fmt::Write as _};

use bitfield::{bitfields, prelude::*};
use cpu::MEMORY;
use gdbstub::target::ext::breakpoints::WatchKind;
use num_conv::prelude::*;

use crate::{
	cpu,
	cpu::csr::{self, AddressTranslationConfig, CSRIndex, CSRInfo, InterruptBits, TrapVector},
	insn::*,
	insn16, insn32,
	mem::{ReadKind, WriteKind},
	regs::{FPRegisters, GPRegisters},
	soft::{FloatStatusControl, float::SoftFloat},
	tracing::*,
	ty::{ExceptionBits, GPRegisterIndex, HartId, HartMode, RiscvExtensions, TrapIdx, TrapKind, TrapRequestGuaranteed},
	util::*,
};

#[derive(Debug)]
pub struct WhiskerHart {
	hart_id: HartId,

	extensions: RiscvExtensions,

	mode: HartMode,
	/// whether this hart is in debug mode, which prevents certain things from trapping or erroring
	/// like memory accesses.
	pub debug: bool,
	/// set to true if the hart has requested to break into the debugger.
	/// reset after the hart has finished processing the request.
	/// this handles debugger interrupts and watchpoints.
	pub requested_break: Option<HartBreakKind>,

	pub registers: GPRegisters,
	pub fp_registers: FPRegisters,

	/// the program counter for the currently executing instruction
	pc: u64,
	/// the value to set pc to at the end of processing the current cycle
	next_pc: u64,

	pub cycles: u64,

	pub csr_info: BTreeMap<CSRIndex, CSRInfo>,

	/// scratch register for trap handlers
	pub mscratch: u64,
	pub mstatus: MStatus,
	pub medeleg: ExceptionBits,
	pub mideleg: InterruptBits,
	pub mie: InterruptBits,
	pub mtvec: TrapVector,

	pub mepc: u64,
	pub mcause: TrapIdx,
	pub mtval: u64,
	pub mip: InterruptBits,

	pub stvec: TrapVector,

	pub sscratch: u64,
	pub sepc: u64,
	pub scause: TrapIdx,
	pub stval: u64,

	pub float_status_control: FloatStatusControl,

	pub translation_config: AddressTranslationConfig,

	/// for debugging
	last_instruction: Option<Instruction>,
}

#[bitfields]
#[derive(Debug, Clone, Copy)]
pub struct MStatus {
	_res_0_0: U1,
	pub sie: bool,
	_res_2_2: U1,
	pub mie: bool,
	_res_4_4: U1,
	pub spie: bool,
	_ube: U1,
	pub mpie: bool,
	pub spp: U1,
	_vs: U2,
	mpp: HartMode,
	_fs: U2,
	_xs: U2,
	_mprv: U1,
	pub sum: bool,
	pub mxr: bool,
	_tvm: U1,
	_tw: U1,
	_tsr: U1,
	_res_23_31: U9,
	_uxl: U2,
	_sxl: U2,
	_sbe: U1,
	_mbe: U1,
	_res_38_62: U25,
	_sd: U1,
}

impl MStatus {
	pub const MASK_S_MODE: u64 = 0b1000_0000_0000_0000_0000_0000_0000_0011_0000_0000_0000_1101_1110_0111_0110_0010;
}

const _: () = {
	assert!(core::mem::size_of::<MStatus>() == core::mem::size_of::<u64>());
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HartBreakKind {
	Watchpoint(WatchKind, u64),
}

impl WhiskerHart {
	pub fn new(hart_id: HartId, extensions: RiscvExtensions, initial_pc: u64) -> Self {
		Self {
			hart_id,
			extensions,

			mode: HartMode::Machine,
			debug: false,
			requested_break: None,

			registers: GPRegisters::default(),
			fp_registers: FPRegisters::default(),
			pc: initial_pc,
			next_pc: 0,

			cycles: 0,

			csr_info: csr::create_info(),

			mscratch: 0,
			mstatus: MStatus::new(),
			medeleg: ExceptionBits::new(),
			mideleg: InterruptBits::new(),
			mie: InterruptBits::new(),
			mtvec: TrapVector::new(),

			mepc: 0,
			// FIXME: should we represent it like this?
			mcause: TrapIdx::exception(0),
			mtval: 0,

			mip: InterruptBits::new(),

			stvec: TrapVector::new(),

			sscratch: 0,
			sepc: 0,
			// FIXME: better sentinel?
			scause: TrapIdx::exception(0),
			stval: 0,

			float_status_control: FloatStatusControl::new(),

			translation_config: AddressTranslationConfig::new(),
			last_instruction: None,
		}
	}

	pub fn hart_id(&self) -> HartId {
		self.hart_id
	}

	pub fn mode(&self) -> HartMode {
		self.mode
	}

	pub fn set_mode(&mut self, mode: HartMode) {
		trace!("hart {:?} setting mode to {:?}", self.hart_id(), mode);
		match mode {
			HartMode::Machine | HartMode::Supervisor | HartMode::User => self.mode = mode,
			HartMode::Hypervisor => unimplemented!("Hypervisor mode is not implemented"),
		}
	}

	pub fn supports_extensions(&self, extensions: RiscvExtensions) -> bool {
		self.extensions.has(extensions)
	}

	pub fn supported_extensions(&self) -> RiscvExtensions {
		self.extensions
	}

	pub fn pc(&self) -> u64 {
		self.pc
	}

	pub fn step(&mut self) {
		if self.debug {
			error!("{:?} still in debug mode at start of step", self.hart_id);
		}

		self.cycles += 1;
		trace!("{:?} cycle {}", self.hart_id, self.cycles);

		// DEBUG: send timer interrupts occasionally
		// if self.cycles > 1024 && self.cycles % 100 == 0 {
		// 	// set the machine timer interrupt pending bit
		// 	let mip = self.read_csr_unchecked(csr::MIP);
		// 	self.write_csr_unchecked(csr::MIP, mip | 1 << 7);
		// }

		// if a trap happened, just update pc and return
		// next cycle will fetch
		if self.check_interrupt_trap() {
			self.pc = self.next_pc;
			self.dump();
			return;
		}

		match self.fetch_instruction() {
			Ok((inst, size)) => {
				trace!("{:#018X}: fetched {:?}", self.pc, inst);
				self.next_pc = self.pc.wrapping_add(size);
				self.last_instruction = Some(inst);
				self.execute_instruction(inst);
			}
			// trap was requested during decoding
			Err(TrapRequestGuaranteed { .. }) => {}
		}

		self.pc = self.next_pc;
	}

	/// requests the specified trap to happen
	/// sets `next_pc` to the appropriate handler for the trap
	pub fn request_trap(&mut self, trap: TrapIdx, tval: u64) -> TrapRequestGuaranteed {
		if self.debug {
			return TrapRequestGuaranteed::__trap_guaranteed_private_new_do_not_use_this_unless_in_trap_handler();
		}

		// FIXME: all the modes?
		warn!(
			"{:?} requesting trap kind cause={:?} tval={:#018X} trapping pc {:#018X} mode={:?}",
			self.hart_id, trap, tval, self.pc, self.mode
		);

		// handle the trap appropriately depending on whether it's delegated
		match self.mode() {
			HartMode::User | HartMode::Supervisor if self.medeleg.is_enabled(trap) || self.mideleg.is_enabled(trap) => {
				self.do_trap_s_mode(trap, tval)
			}
			HartMode::Hypervisor => todo!("H-mode traps not implemented"),
			// traps that were not delegated to lower modes, or the hart is in M mode
			_ => self.do_trap_m_mode(trap, tval),
		}
	}

	fn do_trap_m_mode(&mut self, trap: TrapIdx, tval: u64) -> TrapRequestGuaranteed {
		// interrupts can be disabled or enabled by status bits
		if let TrapKind::Interrupt = trap.kind() {
			if !self.mstatus.get_mie() {
				trace!("skipped trap cause {:?}: machine interrupts were disabled", trap);
				return TrapRequestGuaranteed::__trap_guaranteed_private_new_do_not_use_this_unless_in_trap_handler();
			}

			if !self.mie.is_enabled(trap) {
				trace!("skipped interrupt cause {:?}: cause disabled in MIE CSR", trap);
				return TrapRequestGuaranteed::__trap_guaranteed_private_new_do_not_use_this_unless_in_trap_handler();
			}
		}

		self.mcause = trap;
		self.mtval = tval;

		// set MPIE to the value of MIE before the trap was taken
		// and then disable MIE
		let mut mstatus = self.mstatus;
		let mie = mstatus.get_mie();
		mstatus.set_mpie(mie);
		mstatus.set_mie(false);
		// save previous mode in MPP for restoring in xRET
		mstatus.set_mpp(self.mode());
		self.mstatus = mstatus;

		self.set_mode(HartMode::Machine);

		// save interrupted PC for return
		self.mepc = self.pc;

		let handler = self.mtvec.addr_for_trap(trap);
		trace!("trap handler at {:#018X}", handler);
		self.next_pc = handler;

		TrapRequestGuaranteed::__trap_guaranteed_private_new_do_not_use_this_unless_in_trap_handler()
	}

	fn do_trap_s_mode(&mut self, trap: TrapIdx, tval: u64) -> TrapRequestGuaranteed {
		// interrupts can be disabled or enabled by status bits
		if let TrapKind::Interrupt = trap.kind() {
			if !self.mstatus.get_sie() {
				trace!("skipped trap cause {:?}: machine interrupts were disabled", trap);
				return TrapRequestGuaranteed::__trap_guaranteed_private_new_do_not_use_this_unless_in_trap_handler();
			}

			// it's correct to read MIE here because this path can only be taken
			// if the interrupt was delegated, so it's an S mode visible interrupt
			if !self.mie.is_enabled(trap) {
				trace!("skipped interrupt cause {:?}: cause disabled in SIE CSR", trap);
				return TrapRequestGuaranteed::__trap_guaranteed_private_new_do_not_use_this_unless_in_trap_handler();
			}
		}

		self.scause = trap;
		self.stval = tval;

		// set SPIE to the value of SIE before the trap was taken
		// and then disable SIE
		let mut mstatus = self.mstatus;
		let sie = mstatus.get_sie();
		mstatus.set_spie(sie);
		mstatus.set_sie(false);
		// save previous mode in SPP for restoring in xRET
		mstatus.set_spp(self.mode().bits());
		self.mstatus = mstatus;

		// save interrupted PC for return
		self.sepc = self.pc;

		let handler = self.stvec.addr_for_trap(trap);
		trace!("S mode trap handler at {:#018X}", handler);
		self.next_pc = handler;

		if self.mode() < HartMode::Supervisor {
			self.set_mode(HartMode::Supervisor);
		}

		TrapRequestGuaranteed::__trap_guaranteed_private_new_do_not_use_this_unless_in_trap_handler()
	}

	pub fn check_interrupt_trap(&mut self) -> bool {
		let to_trap = match self.mode() {
			HartMode::User | HartMode::Supervisor => self.check_interrupt_s_mode(),
			HartMode::Hypervisor => todo!("H-mode traps not implemented"),
			HartMode::Machine => self.check_interrupt_m_mode(),
		};

		if to_trap == 0 {
			return false;
		}

		// this subtraction cannot overflow because we know at least one bit is set
		let highest_bit = 63 - to_trap.leading_zeros();
		// implementation specific interrupts have priority from most significant bit to least
		if highest_bit >= 16 {
			self.request_trap(TrapIdx::interrupt(highest_bit.extend::<u64>()), 0);
			return true;
		}

		// standard interrupts have priority:
		// MEI, MSI, MTI, SEI, SSI, STI
		if to_trap & (1 << TrapIdx::MACHINE_EXTERNAL_INTERRUPT.cause()) != 0 {
			self.request_trap(TrapIdx::MACHINE_EXTERNAL_INTERRUPT, 0);
			true
		} else if to_trap & (1 << TrapIdx::MACHINE_SOFTWARE_INTERRUPT.cause()) != 0 {
			self.request_trap(TrapIdx::MACHINE_SOFTWARE_INTERRUPT, 0);
			true
		} else if to_trap & (1 << TrapIdx::MACHINE_TIMER_INTERRUPT.cause()) != 0 {
			self.request_trap(TrapIdx::MACHINE_TIMER_INTERRUPT, 0);
			true
		} else if to_trap & (1 << TrapIdx::SUPERVISOR_EXTERNAL_INTERRUPT.cause()) != 0 {
			self.request_trap(TrapIdx::SUPERVISOR_EXTERNAL_INTERRUPT, 0);
			true
		} else if to_trap & (1 << TrapIdx::SUPERVISOR_SOFTWARE_INTERRUPT.cause()) != 0 {
			self.request_trap(TrapIdx::SUPERVISOR_SOFTWARE_INTERRUPT, 0);
			true
		} else if to_trap & (1 << TrapIdx::SUPERVISOR_TIMER_INTERRUPT.cause()) != 0 {
			self.request_trap(TrapIdx::SUPERVISOR_TIMER_INTERRUPT, 0);
			true
		} else {
			error!("unsupported trap bits {:#018X}", to_trap);
			false
		}
	}

	fn check_interrupt_m_mode(&mut self) -> u64 {
		trace!("checking interrupts on {:?}", self.hart_id());
		let global_enable = self.mstatus.get_mie();
		if !global_enable {
			trace!("M-mode interrupts globally disabled");
			return 0;
		}

		let mip = u64::from_le_bytes(self.mip.inner());
		trace!(" mip currently pending: {:#018X}", mip);
		let mie = u64::from_le_bytes(self.mie.inner());
		let to_trap = mip & mie;
		trace!("pending and enabled: {:#018X}", to_trap);
		to_trap
	}

	fn check_interrupt_s_mode(&mut self) -> u64 {
		trace!("checking interrupts on {:?}", self.hart_id());
		let global_enable = self.mstatus.get_sie();
		if !global_enable {
			trace!("S-mode interrupts globally disabled");
			return 0;
		}

		let sip = csr::read_sip(self);
		trace!(" sip currently pending: {:#018X}", sip);
		let sie = csr::read_sie(self);
		let to_trap = sip & sie;
		trace!("pending and enabled: {:#018X}", to_trap);
		to_trap
	}

	pub fn set_interrupt_pending(&mut self, interrupt: TrapIdx, pending: bool) {
		assert_matches!(interrupt.kind(), TrapKind::Interrupt);

		let mip = u64::from_le_bytes(self.mip.inner());
		let bit_idx = interrupt.cause();
		let mask = 1 << bit_idx;
		let value_bit = u64::from(pending) << bit_idx;
		let mip = (mip & !mask) | value_bit;
		assert!(mip <= (1 << 19), "mip {:#018X} {:?}", mip, interrupt);
		self.mip.set_inner(mip.to_le_bytes());
	}
}

impl WhiskerHart {
	/// tries to fetch an instruction, or returns Err if a trap happened during the fetch
	fn fetch_instruction(&mut self) -> Result<(Instruction, u64), TrapRequestGuaranteed> {
		let support_compressed = self.supports_extensions(RiscvExtensions::COMPRESSED);

		let mem = cpu::MEMORY.wait();

		let (is_compressed, parcel) = mem.read_instruction_parcel(self, self.pc)?;
		let parcel_u16 = parcel.truncate::<u16>();

		// all encodings with the low 16 bits all 0s are invalid.
		// NOTE: the length of an all-zeros instruction is considered
		// to be the length of the smallest supported instruction
		// FIXME: currently we believe this does not matter?
		if parcel_u16 == 0 {
			warn!("tried to execute all 0 instruction at {:#018X}", self.pc);
			return Err(self.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, parcel.extend()));
		}

		if is_compressed {
			if support_compressed {
				if let Some(insn) = insn16::parse(parcel_u16) {
					Ok((insn, 2))
				} else {
					warn!("unable to parse 16 bit instruction {parcel_u16:#06X}");
					Err(self.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, parcel_u16.extend()))
				}
			} else {
				warn!(
					"  tried to execute compressed instruction {:#06X} at {:#018X} when compressed instructions were disabled",
					parcel_u16, self.pc
				);
				Err(self.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, parcel_u16.extend()))
			}
		} else if extract_bits_16(parcel_u16, 2, 4) != 0b111 {
			// FIXME(alignment): parcel must be constructed from 2 reads because when the C extension is
			// enabled, 32 bit instructions may start at addresses only aligned to a multiple of 2.
			match insn32::parse(parcel) {
				Some(insn) => Ok((insn, 4)),
				None => Err(self.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, parcel.extend())),
			}
		} else if extract_bits_16(parcel_u16, 0, 5) == 0b011111 {
			if support_compressed {
				todo!("implement 48bit instruction")
			} else {
				// FIXME: this is probably not the right mtval
				Err(self.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, parcel.extend()))
			}
		} else if extract_bits_16(parcel_u16, 0, 6) == 0b0111111 {
			if support_compressed {
				todo!("implement 64bit instruction")
			} else {
				// FIXME: this is probably not the right mtval
				Err(self.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, parcel.extend()))
			}
		} else {
			// FIXME: this is probably not the right mtval
			Err(self.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, parcel.extend()))
		}
	}

	fn execute_instruction(&mut self, insn: Instruction) {
		let _ = match insn {
			Instruction::Int(insn) => self.execute_i_insn(insn),
			Instruction::Float(insn) if self.supports_extensions(RiscvExtensions::FLOAT) => self.execute_f_insn(insn),
			// FIXME: Figure out what the official ISA defined bit pattern is for Zicsr
			Instruction::Zicsr(insn) => self.execute_csr_insn(insn),
			// We don't have to check compressed here as we handle that in the caller.
			Instruction::Compressed(insn) => self.execute_compressed_insn(insn),
			Instruction::Atomic(insn) if self.supports_extensions(RiscvExtensions::ATOMIC) => {
				self.execute_atomic_insn(insn)
			}
			Instruction::Multipliy(insn) if self.supports_extensions(RiscvExtensions::MULTIPLY) => {
				self.execute_multiply_insn(insn)
			}
			// FIXME for asquared31415
			Instruction::Privileged(insn) => self.execute_privileged_insn(insn),
			// FIXME: Supposed to be the bits of the instruction instead of zero
			_ => Err(self.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0)),
		};
	}
}

// ===========================
// INSTRUCTION IMPLEMENTATION
// ===========================

macro_rules! read_mem_u8 {
	($self:ident, $offset:ident, $kind:path) => {{
		let mem = MEMORY.wait();
		mem.read_u8($self, $offset, $kind)
	}};
}

macro_rules! read_mem_u16 {
	($self:ident, $offset:ident, $kind:path) => {{
		let mem = MEMORY.wait();
		mem.read_u16($self, $offset, $kind)
	}};
}

macro_rules! read_mem_u32 {
	($self:ident, $offset:ident, $kind:path) => {{
		let mem = MEMORY.wait();
		mem.read_u32($self, $offset, $kind)
	}};
}

macro_rules! read_mem_u64 {
	($self:ident, $offset:ident, $kind:path) => {{
		let mem = MEMORY.wait();
		mem.read_u64($self, $offset, $kind)
	}};
}

macro_rules! read_mem_float {
	($self:ident, $offset:ident, $kind:path) => {{
		let mem = MEMORY.wait();
		mem.read_soft_float($self, $offset, $kind)
	}};
}

#[expect(unused, reason = "doubles NYI")]
macro_rules! read_mem_double {
	($self:ident, $offset:ident, $kind:path) => {{
		let mut mem = MEMORY.wait();
		mem.read_soft_double($self, $offset, $kind)
	}};
}

macro_rules! write_mem_u8 {
	($self:ident, $offset:ident, $kind:path, $val:expr) => {{
		let mem = MEMORY.wait();
		mem.write_u8($self, $offset, $kind, $val)
	}};
}

macro_rules! write_mem_u16 {
	($self:ident, $offset:ident, $kind:path, $val:expr) => {{
		let mem = MEMORY.wait();
		mem.write_u16($self, $offset, $kind, $val)
	}};
}

macro_rules! write_mem_u32 {
	($self:ident, $offset:ident, $kind:path, $val:expr) => {{
		let mem = MEMORY.wait();
		mem.write_u32($self, $offset, $kind, $val)
	}};
}

macro_rules! write_mem_u64 {
	($self:ident, $offset:ident, $kind:path, $val:expr) => {{
		let mem = MEMORY.wait();
		mem.write_u64($self, $offset, $kind, $val)
	}};
}

macro_rules! write_mem_float {
	($self:ident, $offset:ident, $kind:path, $val:expr) => {{
		let mem = MEMORY.wait();
		mem.write_soft_float($self, $offset, $kind, $val)
	}};
}

#[expect(unused, reason = "doubles NYI")]
macro_rules! write_mem_double {
	($self:ident, $offset:ident, $kind:path, $val:expr) => {{
		let mut mem = MEMORY.wait();
		mem.write_soft_double($self, $offset, $kind, $val)
	}};
}

impl WhiskerHart {
	fn execute_i_insn(&mut self, insn: IntInstruction) -> Result<(), TrapRequestGuaranteed> {
		match insn {
			IntInstruction::LoadUpperImmediate { dst, val } => {
				self.registers.set(dst, val.cast_unsigned());
			}
			IntInstruction::AddUpperImmediateToPc { dst, val } => {
				self.registers.set(dst, self.pc.wrapping_add_signed(val));
			}
			IntInstruction::StoreByte { dst, dst_offset, src } => {
				let offset = self.registers.get(dst).wrapping_add_signed(dst_offset);
				let val = self.registers.get(src).truncate::<u8>();
				write_mem_u8!(self, offset, WriteKind::Normal, val)?;
			}
			IntInstruction::StoreHalf { dst, dst_offset, src } => {
				let offset = self.registers.get(dst).wrapping_add_signed(dst_offset);
				let val = self.registers.get(src).truncate::<u16>();
				write_mem_u16!(self, offset, WriteKind::Normal, val)?;
			}
			IntInstruction::StoreWord { dst, dst_offset, src } => {
				let offset = self.registers.get(dst).wrapping_add_signed(dst_offset);
				let val = self.registers.get(src).truncate::<u32>();
				write_mem_u32!(self, offset, WriteKind::Normal, val)?;
			}
			IntInstruction::StoreDoubleWord { dst, dst_offset, src } => {
				let offset = self.registers.get(dst).wrapping_add_signed(dst_offset);
				let val = self.registers.get(src);
				write_mem_u64!(self, offset, WriteKind::Normal, val)?;
			}
			IntInstruction::LoadByte { dst, src, src_offset } => {
				let offset = self.registers.get(src).wrapping_add_signed(src_offset);
				let val = read_mem_u8!(self, offset, ReadKind::Normal)?;
				self.registers.set(dst, val.sign_extend::<u64>());
			}
			IntInstruction::LoadHalf { dst, src, src_offset } => {
				let offset = self.registers.get(src).wrapping_add_signed(src_offset);
				let val = read_mem_u16!(self, offset, ReadKind::Normal)?;
				self.registers.set(dst, val.sign_extend::<u64>());
			}
			IntInstruction::LoadWord { dst, src, src_offset } => {
				let offset = self.registers.get(src).wrapping_add_signed(src_offset);
				let val = read_mem_u32!(self, offset, ReadKind::Normal)?;
				self.registers.set(dst, val.sign_extend::<u64>());
			}
			IntInstruction::LoadDoubleWord { dst, src, src_offset } => {
				let offset = self.registers.get(src).wrapping_add_signed(src_offset);
				let val = read_mem_u64!(self, offset, ReadKind::Normal)?;
				self.registers.set(dst, val);
			}
			IntInstruction::LoadByteZeroExtend { dst, src, src_offset } => {
				let offset = self.registers.get(src).wrapping_add_signed(src_offset);
				let val = read_mem_u8!(self, offset, ReadKind::Normal)?;
				self.registers.set(dst, val.extend::<u64>());
			}
			IntInstruction::LoadHalfZeroExtend { dst, src, src_offset } => {
				let offset = self.registers.get(src).wrapping_add_signed(src_offset);
				let val = read_mem_u16!(self, offset, ReadKind::Normal)?;
				self.registers.set(dst, val.extend::<u64>());
			}
			IntInstruction::LoadWordZeroExtend { dst, src, src_offset } => {
				let offset = self.registers.get(src).wrapping_add_signed(src_offset);
				let val = read_mem_u32!(self, offset, ReadKind::Normal)?;
				self.registers.set(dst, val.extend::<u64>());
			}
			IntInstruction::JumpAndLink { link_reg, jmp_off } => {
				self.registers.set(link_reg, self.next_pc);
				// FIXME: if C extension is not enabled, check alignment
				// note: jmp_off is aligned to 2 by nature of its construction during parsing
				self.next_pc = self.pc.wrapping_add_signed(jmp_off);
			}
			IntInstruction::JumpAndLinkRegister {
				link_reg,
				jmp_reg,
				jmp_off,
			} => {
				let jmp_val = self.registers.get(jmp_reg);
				self.registers.set(link_reg, self.next_pc);
				// FIXME: if C extension is not enabled, check alignment
				self.next_pc = jmp_val.wrapping_add_signed(jmp_off) & !1;
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
				let rhs = self.registers.get(rhs).truncate::<u32>();
				let val = lhs.wrapping_shl(rhs);
				self.registers.set(dst, val);
			}
			IntInstruction::ShiftRightLogical { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs).truncate::<u32>();
				let val = lhs.wrapping_shr(rhs);
				self.registers.set(dst, val);
			}
			IntInstruction::ShiftRightArithmetic { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs).cast_signed();
				let rhs = self.registers.get(rhs).truncate::<u32>();
				let val = lhs.wrapping_shr(rhs);
				self.registers.set(dst, val.cast_unsigned());
			}
			IntInstruction::SetLessThan { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs).cast_signed();
				let rhs = self.registers.get(rhs).cast_signed();
				let val = u64::from(lhs < rhs);
				self.registers.set(dst, val);
			}
			IntInstruction::SetLessThanUnsigned { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);
				let val = u64::from(lhs < rhs);
				self.registers.set(dst, val);
			}

			IntInstruction::AddImmediate { dst, lhs, rhs } => {
				self.registers
					.set(dst, self.registers.get(lhs).wrapping_add_signed(rhs));
			}
			IntInstruction::XorImmediate { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				let rhs = rhs.cast_unsigned();
				let val = lhs ^ rhs;
				self.registers.set(dst, val);
			}
			IntInstruction::OrImmediate { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				let rhs = rhs.cast_unsigned();
				let val = lhs | rhs;
				self.registers.set(dst, val);
			}
			IntInstruction::AndImmediate { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				let rhs = rhs.cast_unsigned();
				let val = lhs & rhs;
				self.registers.set(dst, val);
			}
			IntInstruction::ShiftLeftLogicalImmediate { dst, lhs, shift_amt } => {
				let lhs = self.registers.get(lhs);
				let rhs = shift_amt & 0b111111;
				let val = lhs.wrapping_shl(rhs);
				self.registers.set(dst, val);
			}
			IntInstruction::ShiftRightLogicalImmediate { dst, lhs, shift_amt } => {
				let lhs = self.registers.get(lhs);
				let rhs = shift_amt & 0b111111;
				let val = lhs.wrapping_shr(rhs);
				self.registers.set(dst, val);
			}
			IntInstruction::ShiftRightArithmeticImmediate { dst, lhs, shift_amt } => {
				let lhs = self.registers.get(lhs).cast_signed();
				let rhs = shift_amt & 0b111111;
				let val = lhs.wrapping_shr(rhs);
				self.registers.set(dst, val.cast_unsigned());
			}
			IntInstruction::SetLessThanImmediate { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs).cast_signed();
				let val = u64::from(lhs < rhs);
				self.registers.set(dst, val);
			}
			IntInstruction::SetLessThanUnsignedImmediate { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				let rhs = rhs.cast_unsigned();
				let val = u64::from(lhs < rhs);
				self.registers.set(dst, val);
			}
			IntInstruction::AddImmediateWord { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs).truncate::<u32>();
				let result = lhs.wrapping_add_signed(rhs);
				self.registers.set(dst, result.sign_extend::<u64>());
			}
			// shift word instructions all do the shift as 32 bits, and then sign extend the result
			IntInstruction::ShiftLeftLogicalImmediateWord { dst, lhs, shift_amt } => {
				let lhs = self.registers.get(lhs).truncate::<u32>();
				let result = lhs.wrapping_shl(shift_amt);
				self.registers.set(dst, result.sign_extend::<u64>());
			}
			IntInstruction::ShiftRightLogicalImmediateWord { dst, lhs, shift_amt } => {
				let lhs = self.registers.get(lhs).truncate::<u32>();
				let result = lhs.wrapping_shr(shift_amt);
				self.registers.set(dst, result.sign_extend::<u64>());
			}
			IntInstruction::ShiftRightArithmeticImmediateWord { dst, lhs, shift_amt } => {
				let lhs = self.registers.get(lhs).truncate::<u32>().cast_signed();
				let result = lhs.wrapping_shr(shift_amt);
				self.registers.set(dst, result.sign_extend::<u64>());
			}

			// ============
			// BRANCH
			// ============
			IntInstruction::BranchEqual { lhs, rhs, imm } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);
				if lhs == rhs {
					// FIXME: if C extension is not enabled, check alignment
					self.next_pc = self.pc.wrapping_add_signed(imm);
				}
			}
			IntInstruction::BranchNotEqual { lhs, rhs, imm } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);
				if lhs != rhs {
					// FIXME: if C extension is not enabled, check alignment
					self.next_pc = self.pc.wrapping_add_signed(imm);
				}
			}
			IntInstruction::BranchLessThan { lhs, rhs, imm } => {
				let lhs = self.registers.get(lhs).cast_signed();
				let rhs = self.registers.get(rhs).cast_signed();
				if lhs < rhs {
					// FIXME: if C extension is not enabled, check alignment
					self.next_pc = self.pc.wrapping_add_signed(imm);
				}
			}
			IntInstruction::BranchGreaterEqual { lhs, rhs, imm } => {
				let lhs = self.registers.get(lhs).cast_signed();
				let rhs = self.registers.get(rhs).cast_signed();
				if lhs >= rhs {
					// FIXME: if C extension is not enabled, check alignment
					self.next_pc = self.pc.wrapping_add_signed(imm);
				}
			}
			IntInstruction::BranchLessThanUnsigned { lhs, rhs, imm } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);
				if lhs < rhs {
					// FIXME: if C extension is not enabled, check alignment
					self.next_pc = self.pc.wrapping_add_signed(imm);
				}
			}
			IntInstruction::BranchGreaterEqualUnsigned { lhs, rhs, imm } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);
				if lhs >= rhs {
					// FIXME: if C extension is not enabled, check alignment
					self.next_pc = self.pc.wrapping_add_signed(imm);
				}
			}

			IntInstruction::AddWord { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs).truncate::<u32>();
				let rhs = self.registers.get(rhs).truncate::<u32>();
				let result = lhs.wrapping_add(rhs);
				self.registers.set(dst, result.sign_extend::<u64>());
			}
			IntInstruction::SubWord { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs).truncate::<u32>();
				let rhs = self.registers.get(rhs).truncate::<u32>();
				let result = lhs.wrapping_sub(rhs);
				self.registers.set(dst, result.sign_extend::<u64>());
			}
			// shift word with a register shift amount only use the low 5 bits of the rhs as the amount
			// these instructions also all sign extend the 32 bit result
			IntInstruction::ShiftLeftLogicalWord { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs).truncate::<u32>();
				let shamt = (self.registers.get(rhs) & 0b11111).truncate::<u32>();
				let result = lhs.wrapping_shl(shamt);
				self.registers.set(dst, result.sign_extend::<u64>());
			}
			IntInstruction::ShiftRightLogicalWord { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs).truncate::<u32>();
				let shamt = (self.registers.get(rhs) & 0b11111).truncate::<u32>();
				let result = lhs.wrapping_shr(shamt);
				self.registers.set(dst, result.sign_extend::<u64>());
			}
			IntInstruction::ShiftRightArithmeticWord { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs).truncate::<u32>().cast_signed();
				let shamt = (self.registers.get(rhs) & 0b11111).truncate::<u32>();
				let result = lhs.wrapping_shr(shamt);
				self.registers.set(dst, result.sign_extend::<u64>());
			}

			// we don't do reordering, fence is a no-op
			IntInstruction::Fence { .. } => {}
			IntInstruction::InstructionFence => {
				MEMORY.wait().instruction_parcel_cache.write().clear();
			}

			// =========
			// SYSTEM
			// =========
			IntInstruction::ECall => {
				let trap = match self.mode() {
					HartMode::User => TrapIdx::ECALL_UMODE,
					HartMode::Supervisor => TrapIdx::ECALL_SMODE,
					HartMode::Hypervisor => todo!(),
					HartMode::Machine => TrapIdx::ECALL_MMODE,
				};
				self.request_trap(trap, 0);
			}
			IntInstruction::EBreak => {
				// TODO: should this do anything else?
				self.request_trap(TrapIdx::BREAKPOINT, 0);
			}
		}
		Ok(())
	}

	#[allow(unused_variables, reason = "FIXME: Implement unfinished instructions")]
	fn execute_f_insn(&mut self, insn: FloatInstruction) -> Result<(), TrapRequestGuaranteed> {
		match insn {
			FloatInstruction::LoadWord { dst, src, src_offset } => {
				let offset = self.registers.get(src).wrapping_add_signed(src_offset);
				let val = read_mem_float!(self, offset, ReadKind::Normal)?;
				self.fp_registers.set_float(dst, val);
			}
			FloatInstruction::StoreWord { dst, dst_offset, src } => {
				let offset = self.registers.get(dst).wrapping_add_signed(dst_offset);
				let val = self.fp_registers.get_float(src);
				write_mem_float!(self, offset, WriteKind::Normal, val)?;
			}
			FloatInstruction::AddSingle { dst, lhs, rhs, rm } => {
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);
				let result = lhs.add(rhs, rm, self);
				self.fp_registers.set_float(dst, result);
			}
			FloatInstruction::SubSingle { dst, lhs, rhs, rm } => {
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);
				let result = lhs.sub(rhs, rm, self);
				self.fp_registers.set_float(dst, result);
			}
			FloatInstruction::MulAddSingle {
				dst,
				mul_lhs,
				mul_rhs,
				add,
				rm,
			} => {
				let mul_lhs = self.fp_registers.get_float(mul_lhs);
				let mul_rhs = self.fp_registers.get_float(mul_rhs);
				let add = self.fp_registers.get_float(add);
				let result = mul_lhs.mul_add(mul_rhs, add, rm, self);
				self.fp_registers.set_float(dst, result);
			}
			FloatInstruction::MulSubSingle {
				dst,
				mul_lhs,
				mul_rhs,
				sub,
				rm,
			} => {
				let mul_lhs = self.fp_registers.get_float(mul_lhs);
				let mul_rhs = self.fp_registers.get_float(mul_rhs);
				let sub = self.fp_registers.get_float(sub);
				let result = mul_lhs.mul_sub(mul_rhs, sub, rm, self);
				self.fp_registers.set_float(dst, result);
			}
			FloatInstruction::NegMulAddSingle {
				dst,
				mul_lhs,
				mul_rhs,
				add,
				rm,
			} => {
				let mul_lhs = self.fp_registers.get_float(mul_lhs).set_sign(false);
				let mul_rhs = self.fp_registers.get_float(mul_rhs);
				let add = self.fp_registers.get_float(add);
				let result = mul_lhs.mul_add(mul_rhs, add, rm, self);
				self.fp_registers.set_float(dst, result);
			}
			FloatInstruction::NegMulSubSingle {
				dst,
				mul_lhs,
				mul_rhs,
				sub,
				rm,
			} => {
				let mul_lhs = self.fp_registers.get_float(mul_lhs).set_sign(false);
				let mul_rhs = self.fp_registers.get_float(mul_rhs);
				let sub = self.fp_registers.get_float(sub);
				let result = mul_lhs.mul_sub(mul_rhs, sub, rm, self);
				self.fp_registers.set_float(dst, result);
			}
			FloatInstruction::MulSingle { dst, lhs, rhs, rm } => {
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);
				let result = lhs.mul(rhs, rm, self);
				self.fp_registers.set_float(dst, result);
			}
			FloatInstruction::DivSingle { dst, lhs, rhs, rm } => {
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);
				let result = lhs.div(rhs, rm, self);
				self.fp_registers.set_float(dst, result);
			}
			FloatInstruction::SqrtSingle { dst, val, rm } => {
				let result = self.fp_registers.get_float(val).sqrt(rm, self);
				self.fp_registers.set_float(dst, result);
			}
			FloatInstruction::SignInjectionSingle { dst, lhs, rhs } => {
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);
				self.fp_registers.set_float(dst, lhs.set_sign(rhs.sign()));
			}
			FloatInstruction::SignNotInjectionSingle { dst, lhs, rhs } => {
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);
				self.fp_registers.set_float(dst, lhs.set_sign(!rhs.sign()));
			}
			FloatInstruction::SignXorInjectionSingle { dst, lhs, rhs } => {
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);
				self.fp_registers.set_float(dst, lhs.set_sign(lhs.sign() ^ rhs.sign()));
			}
			FloatInstruction::MinSingle { dst, lhs, rhs } => {
				// TODO: Fix this implementation
				// PAGE: 115
				warn!("FMIN.S Implementation is incorrect");
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);
				if lhs < rhs {
					self.fp_registers.set_float(dst, lhs);
				} else {
					self.fp_registers.set_float(dst, rhs);
				}
			}
			FloatInstruction::MaxSingle { dst, lhs, rhs } => {
				// TODO: Fix this implementation
				// PAGE: 115
				warn!("FMAX.S Implementation is incorrect");
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);
				if lhs > rhs {
					self.fp_registers.set_float(dst, lhs);
				} else {
					self.fp_registers.set_float(dst, rhs);
				}
			}
			FloatInstruction::EqualSingle { dst, lhs, rhs } => {
				//FEQ.S performs a quiet comparison:
				//it only sets the invalid operation exception flag if either input is a signaling NaN. For all three
				//instructions, the result is 0 if either operand is NaN.

				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);

				// the partial_cmp here returns None if either lhs or rhs is NaN
				if let Some(cmp) = lhs.partial_cmp(&rhs) {
					self.registers.set(dst, u64::from(cmp == Ordering::Equal));
				} else {
					// if any input was NaN, the output is 0
					self.registers.set(dst, 0);
					// if either input was sNaN, write invalid operation
					if lhs.is_snan() || rhs.is_snan() {
						self.float_status_control.set_invalid_operation(true);
					}
				}
			}
			FloatInstruction::LessThanSingle { dst, lhs, rhs } => {
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);

				// the partial_cmp here returns None if either lhs or rhs is nan
				if let Some(cmp) = lhs.partial_cmp(&rhs) {
					self.registers.set(dst, u64::from(cmp == Ordering::Less));
				} else {
					self.registers.set(dst, 0);
					self.float_status_control.set_invalid_operation(true);
				}
			}
			FloatInstruction::LessOrEqualSingle { dst, lhs, rhs } => {
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);

				// the partial_cmp here returns None if either lhs or rhs is nan
				if let Some(cmp) = lhs.partial_cmp(&rhs) {
					self.registers
						.set(dst, u64::from(matches!(cmp, Ordering::Less | Ordering::Equal)));
				} else {
					self.registers.set(dst, 0);
					self.float_status_control.set_invalid_operation(true);
				}
			}
			FloatInstruction::ConvertSingleToWord { dst, src, rm } => todo!("convert single to word"),
			FloatInstruction::ConvertSingleToWordUnsigned { dst, src, rm } => todo!("convert single to word unsigned"),
			FloatInstruction::ConvertSingleToDoubleWord { dst, src, rm } => todo!("convert single to double word"),
			FloatInstruction::ConvertSingleToDoubleWordUnsigned { dst, src, rm } => {
				todo!("convert single to double word unsigned")
			}
			FloatInstruction::ConvertWordToSingle { dst, src, rm } => todo!("convert word to single"),
			FloatInstruction::ConvertWordUnsignedToSingle { dst, src, rm } => todo!("convert word unsigned to single"),
			FloatInstruction::ConvertDoubleWordToSingle { dst, src, rm } => todo!("convert double word to single"),
			FloatInstruction::ConvertDoubleWordUnsignedToSingle { dst, src, rm } => {
				todo!("convert double word unsigned to single")
			}
			FloatInstruction::MoveSingleToInteger { dst, src } => {
				// the high 32 bits of the destination register are filled with copies of the float's sign bit
				let val = self.fp_registers.get_raw(src).truncate::<u32>().sign_extend::<u64>();
				self.registers.set(dst, val);
			}
			FloatInstruction::MoveIntegerToSingle { dst, src } => {
				let val = self.registers.get(src).truncate::<u32>();
				self.fp_registers.set_float(dst, SoftFloat::from_u32(val));
			}
			FloatInstruction::Class { dst, src } => {
				let val = self.fp_registers.get_float(src);
				let class = val.fclass();
				self.registers.set(dst, class.to_shift().extend::<u64>());
			}
		}
		Ok(())
	}

	fn execute_csr_insn(&mut self, insn: CSRInstruction) -> Result<(), TrapRequestGuaranteed> {
		// FIXME: ordering of effects on registers and traps???
		match insn {
			CSRInstruction::CSRReadWrite { dst, src, csr } => {
				let token = self.csr_require_rw(csr)?;
				let new_val = self.registers.get(src);
				// reads dont happen when dst is zero
				if dst != GPRegisterIndex::ZERO {
					let val = self.read_csr(&token);
					self.registers.set(dst, val);
				}
				self.write_csr(&token, new_val);
			}
			CSRInstruction::CSRReadAndSet { dst, mask, csr } => {
				// we MUST NOT check for writability if the mask register is x0
				if mask == GPRegisterIndex::ZERO {
					let token = self.csr_require_ro(csr)?;
					let val = self.read_csr(&token);
					self.registers.set(dst, val);
				} else {
					let token = self.csr_require_rw(csr)?;

					let val = self.read_csr(&token);
					let mask = self.registers.get(mask);
					self.registers.set(dst, val);
					self.write_csr(&token, val | mask);
				}
			}
			CSRInstruction::CSRReadAndClear { dst, mask, csr } => {
				// we MUST NOT check for writability if the mask register is x0
				if mask == GPRegisterIndex::ZERO {
					let token = self.csr_require_ro(csr)?;
					let val = self.read_csr(&token);
					self.registers.set(dst, val);
				} else {
					let token = self.csr_require_rw(csr)?;
					let val = self.read_csr(&token);
					let mask = self.registers.get(mask);
					self.registers.set(dst, val);
					self.write_csr(&token, val & mask);
				}
			}
			CSRInstruction::CSRReadWriteImm { dst, imm, csr } => {
				let token = self.csr_require_rw(csr)?;
				// reads dont happen when dst is zero
				if dst != GPRegisterIndex::ZERO {
					let val = self.read_csr(&token);
					self.registers.set(dst, val);
				}
				self.write_csr(&token, imm);
			}
			CSRInstruction::CSRReadAndSetImm { dst, mask, csr } => {
				// we MUST NOT check for writability if the mask is 0
				if mask != 0 {
					let token = self.csr_require_rw(csr)?;
					let val = self.read_csr(&token);
					self.registers.set(dst, val);
					self.write_csr(&token, val | mask);
				} else {
					let token = self.csr_require_ro(csr)?;
					let val = self.read_csr(&token);
					self.registers.set(dst, val);
				}
			}
			CSRInstruction::CSRReadAndClearImm { dst, mask, csr } => {
				// we MUST NOT check for writability if the mask is 0
				if mask != 0 {
					let token = self.csr_require_rw(csr)?;
					let val = self.read_csr(&token);
					self.registers.set(dst, val);
					self.write_csr(&token, val & mask);
				} else {
					let token = self.csr_require_ro(csr)?;
					let val = self.read_csr(&token);
					self.registers.set(dst, val);
				}
			}
		}
		Ok(())
	}

	#[allow(
		clippy::unnecessary_wraps,
		clippy::unused_self,
		reason = "Consistency with other execute functions"
	)]
	fn execute_compressed_insn(&mut self, insn: CompressedInstruction) -> Result<(), TrapRequestGuaranteed> {
		match insn {
			// this nop is special in that it's designated as an explicit NOP for future standard use
			// so it cannot be combined into an integer instruction
			// we may or may not want to optimize this for hints?
			CompressedInstruction::Nop => {}
		}
		Ok(())
	}

	fn execute_atomic_insn(&mut self, insn: AtomicInstruction) -> Result<(), TrapRequestGuaranteed> {
		// TODO(atomic): For now we'll be ignoring the aq: _ and rl: _ bits as it requires fencing logic
		// and other things we do not currently implement.
		match insn {
			AtomicInstruction::LoadReservedWord { src, dst, _aq, _rl } => {
				let addr = self.registers.get(src);
				if addr % 4 != 0 {
					return Err(self.request_trap(TrapIdx::LOAD_ADDR_MISALIGNED, addr));
				}

				let memory = MEMORY.wait();
				let val = memory.load_reserved_word(self, addr)?;
				self.registers.set(dst, val.extend());
			}
			AtomicInstruction::StoreConditionalWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				if addr % 4 != 0 {
					return Err(self.request_trap(TrapIdx::STORE_ADDR_MISALIGNED, addr));
				}

				let val = self.registers.get(src2).truncate::<u32>();

				let memory = MEMORY.wait();
				let success = memory.store_conditional_word(self, addr, val)?;
				self.registers.set(dst, u64::from(!success));
			}

			AtomicInstruction::SwapWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let mem = MEMORY.wait();

				let addr = self.registers.get(src1);
				mem.atomic_op_word(self, addr, |hart, word| {
					// swap src2 to (src1)
					let src2_val = hart.registers.get(src2).truncate::<u32>();

					// put (src1) value into rd
					hart.registers.set(dst, word.sign_extend::<u64>());
					Some(src2_val)
				})?;
			}
			AtomicInstruction::AddWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let mem = MEMORY.wait();

				let addr = self.registers.get(src1);
				mem.atomic_op_word(self, addr, |this, word| {
					// add src2 value to (src1)
					let src2_val = this.registers.get(src2).truncate::<u32>();
					let new_val = word.wrapping_add(src2_val);

					// put (src1) value into rd
					this.registers.set(dst, word.sign_extend::<u64>());
					Some(new_val)
				})?;
			}
			AtomicInstruction::XorWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let mem = MEMORY.wait();

				let addr = self.registers.get(src1);

				mem.atomic_op_word(self, addr, |this, word| {
					// xor src2 value with (src1)
					let src2_val = this.registers.get(src2).truncate::<u32>();
					let new_val = word ^ src2_val;

					// put (src1) value into rd
					this.registers.set(dst, word.sign_extend::<u64>());
					Some(new_val)
				})?;
			}
			AtomicInstruction::AndWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let mem = MEMORY.wait();

				let addr = self.registers.get(src1);
				mem.atomic_op_word(self, addr, |this, word| {
					// and src2 value with (src1)
					let src2_val = this.registers.get(src2).truncate::<u32>();
					let new_val = word & src2_val;

					// put (src1) value into rd
					this.registers.set(dst, word.sign_extend::<u64>());
					Some(new_val)
				})?;
			}
			AtomicInstruction::OrWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let mem = MEMORY.wait();

				let addr = self.registers.get(src1);
				mem.atomic_op_word(self, addr, |this, word| {
					// or src2 value with (src1)
					let src2_val = this.registers.get(src2).truncate::<u32>();
					let new_val = word | src2_val;

					// put (src1) value into rd
					this.registers.set(dst, word.sign_extend::<u64>());
					Some(new_val)
				})?;
			}
			AtomicInstruction::MinWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let mem = MEMORY.wait();

				let addr = self.registers.get(src1);
				mem.atomic_op_word(self, addr, |this, word| {
					// min of src2 value and (src1) (signed)
					let src2_val = this.registers.get(src2).truncate::<u32>().cast_signed();
					let new_val = std::cmp::min(word.cast_signed(), src2_val).cast_unsigned();

					// put (src1) value into rd
					this.registers.set(dst, word.sign_extend::<u64>());
					Some(new_val)
				})?;
			}
			AtomicInstruction::MaxWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let mem = MEMORY.wait();

				let addr = self.registers.get(src1);
				mem.atomic_op_word(self, addr, |this, word| {
					// max of src2 value and (src1) (signed)
					let src2_val = this.registers.get(src2).truncate::<u32>().cast_signed();
					let new_val = std::cmp::max(word.cast_signed(), src2_val).cast_unsigned();

					// put (src1) value into rd
					this.registers.set(dst, word.sign_extend::<u64>());
					Some(new_val)
				})?;
			}
			AtomicInstruction::MinUnsignedWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let mem = MEMORY.wait();

				let addr = self.registers.get(src1);
				mem.atomic_op_word(self, addr, |this, word| {
					// min of src2 value and (src1) (unsigned)
					let src2_val = this.registers.get(src2).truncate::<u32>();
					let new_val = std::cmp::min(word, src2_val);

					// put (src1) value into rd
					this.registers.set(dst, word.sign_extend::<u64>());
					Some(new_val)
				})?;
			}
			AtomicInstruction::MaxUnsignedWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let mem = MEMORY.wait();

				let addr = self.registers.get(src1);
				mem.atomic_op_word(self, addr, |this, word| {
					// max of src2 value and (src1) (unsigned)
					let src2_val = this.registers.get(src2).truncate::<u32>();
					let new_val = std::cmp::max(word, src2_val);

					// put (src1) value into rd
					this.registers.set(dst, word.sign_extend::<u64>());
					Some(new_val)
				})?;
			}
			AtomicInstruction::LoadReservedDoubleWord { src, dst, _aq, _rl } => {
				let addr = self.registers.get(src);
				let memory = MEMORY.wait();
				let val = memory.load_reserved_dword(self, addr)?;
				self.registers.set(dst, val);
			}
			AtomicInstruction::StoreConditionalDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				let val = self.registers.get(src2);

				let memory = MEMORY.wait();
				let success = memory.store_conditional_dword(self, addr, val)?;
				self.registers.set(dst, u64::from(!success));
			}

			AtomicInstruction::SwapDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let mem = MEMORY.wait();

				let addr = self.registers.get(src1);
				mem.atomic_op_dword(self, addr, |this, dword| {
					// swap src2 to (src1)
					let src2_val = this.registers.get(src2);

					// put (src1) value into rd
					this.registers.set(dst, dword);
					Some(src2_val)
				})?;
			}
			AtomicInstruction::AddDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let mem = MEMORY.wait();

				let addr = self.registers.get(src1);
				mem.atomic_op_dword(self, addr, |this, dword| {
					// add src2 value to (src1)
					let src2_val = this.registers.get(src2);
					let new_val = dword.wrapping_add(src2_val);

					// put (src1) value into rd
					this.registers.set(dst, dword);
					Some(new_val)
				})?;
			}
			AtomicInstruction::XorDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let mem = MEMORY.wait();

				let addr = self.registers.get(src1);
				mem.atomic_op_dword(self, addr, |this, dword| {
					// xor src2 value with (src1)
					let src2_val = this.registers.get(src2);
					let new_val = dword ^ src2_val;

					// put (src1) value into rd
					this.registers.set(dst, dword);
					Some(new_val)
				})?;
			}
			AtomicInstruction::AndDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let mem = MEMORY.wait();

				let addr = self.registers.get(src1);
				mem.atomic_op_dword(self, addr, |this, dword| {
					// and src2 value with (src1)
					let src2_val = this.registers.get(src2);
					let new_val = dword & src2_val;

					// put (src1) value into rd
					this.registers.set(dst, dword);
					Some(new_val)
				})?;
			}
			AtomicInstruction::OrDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let mem = MEMORY.wait();

				let addr = self.registers.get(src1);
				mem.atomic_op_dword(self, addr, |this, dword| {
					// or src2 value with (src1)
					let src2_val = this.registers.get(src2);
					let new_val = dword | src2_val;

					// put (src1) value into rd
					this.registers.set(dst, dword);
					Some(new_val)
				})?;
			}
			AtomicInstruction::MinDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let mem = MEMORY.wait();

				let addr = self.registers.get(src1);
				mem.atomic_op_dword(self, addr, |this, dword| {
					// min of src2 value and (src1) (signed)
					let src2_val = this.registers.get(src2).cast_signed();
					let new_val = std::cmp::min(dword.cast_signed(), src2_val).cast_unsigned();

					// put (src1) value into rd
					this.registers.set(dst, dword);
					Some(new_val)
				})?;
			}
			AtomicInstruction::MaxDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let mem = MEMORY.wait();

				let addr = self.registers.get(src1);
				mem.atomic_op_dword(self, addr, |this, dword| {
					// max of src2 value and (src1) (signed)
					let src2_val = this.registers.get(src2).cast_signed();
					let new_val = std::cmp::max(dword.cast_signed(), src2_val).cast_unsigned();

					// put (src1) value into rd
					this.registers.set(dst, dword);
					Some(new_val)
				})?;
			}
			AtomicInstruction::MinUnsignedDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let mem = MEMORY.wait();

				let addr = self.registers.get(src1);
				mem.atomic_op_dword(self, addr, |this, dword| {
					// min of src2 value and (src1) (unsigned)
					let src2_val = this.registers.get(src2);
					let new_val = std::cmp::min(dword, src2_val);

					// put (src1) value into rd
					this.registers.set(dst, dword);
					Some(new_val)
				})?;
			}
			AtomicInstruction::MaxUnsignedDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let mem = MEMORY.wait();

				let addr = self.registers.get(src1);
				mem.atomic_op_dword(self, addr, |this, dword| {
					// max of src2 value and (src1) (unsigned)
					let src2_val = this.registers.get(src2);
					let new_val = std::cmp::max(dword, src2_val);

					// put (src1) value into rd
					this.registers.set(dst, dword);
					Some(new_val)
				})?;
			}
		}
		Ok(())
	}

	#[allow(clippy::unnecessary_wraps, reason = "Consistency with other execute functions")]
	fn execute_multiply_insn(&mut self, insn: MultiplyInstruction) -> Result<(), TrapRequestGuaranteed> {
		match insn {
			MultiplyInstruction::Multiply { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);

				self.registers.set(dst, lhs.wrapping_mul(rhs));
			}
			MultiplyInstruction::MultiplyHigh { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs).cast_signed().extend::<i128>();
				let rhs = self.registers.get(rhs).cast_signed().extend::<i128>();

				// full 128bit signed mul
				let product = lhs * rhs;
				let hi_bits = (product >> 64).cast_unsigned().truncate::<u64>();

				self.registers.set(dst, hi_bits);
			}
			MultiplyInstruction::MultiplyHighSignedUnsigned { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs).cast_signed().extend::<i128>();
				let rhs = self.registers.get(rhs).zero_extend::<i128>();

				let product = lhs * rhs;
				let hi_bits = (product >> 64).cast_unsigned().truncate::<u64>();

				self.registers.set(dst, hi_bits);
			}
			MultiplyInstruction::MultiplyHighUnsigned { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs).extend::<u128>();
				let rhs = self.registers.get(rhs).extend::<u128>();

				let product = lhs * rhs;
				let hi_bits = (product >> 64).truncate::<u64>();

				self.registers.set(dst, hi_bits);
			}
			MultiplyInstruction::Divide { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs).cast_signed();
				let rhs = self.registers.get(rhs).cast_signed();

				let result = if rhs == 0 {
					-1i64 // div by zero returns -1
				} else if lhs == i64::MIN && rhs == -1 {
					lhs // overflow returns dividend
				} else {
					lhs.wrapping_div(rhs)
				};

				self.registers.set(dst, result.cast_unsigned());
			}
			MultiplyInstruction::DivideUnsigned { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);

				// div by zero returns 0b1111111111...
				let result = if rhs == 0 { u64::MAX } else { lhs.wrapping_div(rhs) };

				self.registers.set(dst, result);
			}
			MultiplyInstruction::Remainder { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs).cast_signed();
				let rhs = self.registers.get(rhs).cast_signed();

				let result = if rhs == 0 {
					lhs // rem by zero returns dividend
				} else if lhs == i64::MIN && rhs == -1 {
					0 // overflow returns nothing
				} else {
					lhs.wrapping_rem(rhs)
				};

				self.registers.set(dst, result.cast_unsigned());
			}
			MultiplyInstruction::RemainderUnsigned { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);

				// rem by zero returns dividend
				let result = if rhs == 0 { lhs } else { lhs.wrapping_rem(rhs) };

				self.registers.set(dst, result);
			}

			MultiplyInstruction::MultiplyWord { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs).truncate::<u32>().cast_signed();
				let rhs = self.registers.get(rhs).truncate::<u32>().cast_signed();

				let result = lhs * rhs;

				self.registers.set(dst, result.sign_extend::<u64>());
			}
			MultiplyInstruction::DivideWord { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs).truncate::<u32>().cast_signed();
				let rhs = self.registers.get(rhs).truncate::<u32>().cast_signed();

				let result = if rhs == 0 {
					-1i32 // div by zero returns -1
				} else if lhs == i32::MIN && rhs == -1 {
					lhs // overflow returns dividend
				} else {
					lhs.wrapping_div(rhs)
				};

				self.registers.set(dst, result.sign_extend::<u64>());
			}
			MultiplyInstruction::DivideUnsignedWord { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs).truncate::<u32>();
				let rhs = self.registers.get(rhs).truncate::<u32>();

				// div by zero returns 0b111111111...
				let result = if rhs == 0 { u32::MAX } else { lhs.wrapping_div(rhs) };

				self.registers.set(dst, result.sign_extend::<u64>());
			}
			MultiplyInstruction::RemainderWord { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs).truncate::<u32>().cast_signed();
				let rhs = self.registers.get(rhs).truncate::<u32>().cast_signed();

				let result = if rhs == 0 {
					lhs // rem by zero returns dividend
				} else if lhs == i32::MIN && rhs == -1 {
					0 // overflow returns nothing
				} else {
					lhs.wrapping_rem(rhs)
				};

				self.registers.set(dst, result.sign_extend::<u64>());
			}
			MultiplyInstruction::RemainderUnsignedWord { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs).truncate::<u32>();
				let rhs = self.registers.get(rhs).truncate::<u32>();

				// rem by zero returns dividend
				let result = if rhs == 0 { lhs } else { lhs.wrapping_rem(rhs) };

				self.registers.set(dst, result.sign_extend::<u64>());
			}
		}
		Ok(())
	}

	#[allow(clippy::unnecessary_wraps, reason = "Consistency with other execute functions")]
	fn execute_privileged_insn(&mut self, insn: PrivilegedInstruction) -> Result<(), TrapRequestGuaranteed> {
		match insn {
			PrivilegedInstruction::Mret => {
				if self.mode() < HartMode::Machine {
					// FIXME: what val should this be?
					self.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
				}

				let mut mstatus = self.mstatus;
				let mpie = mstatus.get_mpie();
				let new_priv = mstatus.get_mpp();

				// MIE = MPIE; MPIE = 1
				mstatus.set_mie(mpie);
				mstatus.set_mpie(true);

				// set mode to MPP
				trace!("MRET setting mode to {:?}", new_priv);
				self.set_mode(new_priv);

				// set MPP to lowest supported mode
				// FIXME (U mode): use U-mode here
				mstatus.set_mpp(HartMode::Supervisor);
				self.mstatus = mstatus;

				self.next_pc = self.mepc;
			}
			PrivilegedInstruction::Sret => {
				if self.mode() < HartMode::Supervisor {
					// FIXME: what val should this be?
					self.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
				}

				let mut mstatus = self.mstatus;
				let spie = mstatus.get_spie();
				let new_priv = match mstatus.get_spp() {
					0 => HartMode::User,
					1 => HartMode::Supervisor,
					_ => unreachable!(),
				};

				// SIE = SPIE; SPIE = 1
				mstatus.set_sie(spie);
				mstatus.set_spie(true);

				// set mode to SPP
				trace!("SRET setting mode to {:?}", new_priv);
				self.set_mode(new_priv);

				// set SPP to lowest supported mode
				mstatus.set_spp(0);
				self.mstatus = mstatus;

				self.next_pc = self.sepc;
			}
			PrivilegedInstruction::WaitForInterrupt => {
				// it's legal for WFI to be a no-op
				// FIXME: maybe make this more efficient tho?
			}
			PrivilegedInstruction::Sfence { asid, vaddr } => {
				if self.mode() < HartMode::Supervisor {
					// FIXME: what val should this be?
					self.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
				}

				let vaddr = self.registers.get(vaddr);
				let asid = self.registers.get(asid);

				MEMORY.wait().clear_vm_cache(asid, vaddr);
			}
		}
		Ok(())
	}
}

// ===================
// DEBUG
// ===================
impl WhiskerHart {
	pub fn request_watchpoint(&mut self, watch_kind: WatchKind, addr: u64) {
		self.requested_break = Some(HartBreakKind::Watchpoint(watch_kind, addr));
	}

	pub fn set_pc_debug(&mut self, pc: u64) {
		self.pc = pc;
	}

	pub fn dump(&mut self) -> String {
		let mut out = format!("{:?} cycle {}\n", self.hart_id, self.cycles);
		if let Some(i) = &self.last_instruction {
			writeln!(&mut out, "  executed {:?}\n\n", i).unwrap();
		}

		writeln!(&mut out, "  state after cycle:\n").unwrap();
		writeln!(&mut out, "    pc: {:#018X}\n", self.pc).unwrap();
		let regs = self.registers.regs();
		for idx in 0..32 {
			let val = regs[usize::from(idx)];
			// we do this for pretty display purposes
			let idx = GPRegisterIndex::new(idx).unwrap();
			writeln!(&mut out, "  {:>4}: {val:#018X} ({val:})", idx.display(),).unwrap();
		}

		writeln!(&mut out).unwrap();
		/*
		let fpregs = self.fp_registers.get_all_raw();
		for idx in 0..32 {
			let val = u64::from_le_bytes(fpregs[idx].to_le_bytes());
			writeln!(
				&mut out,
				"  {:>4}: {:#018X} (f64: {}, f32: {})",
				format!("fp{idx}"),
				val,
				f64::from_bits(val),
				f32::from_bits(val as u32)
			)
			.unwrap();
		}
		*/
		out.push_str("\n\n");

		out
	}
}
