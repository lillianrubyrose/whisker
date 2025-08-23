use std::collections::BTreeMap;
use std::fmt::Debug;
use std::ops::Deref;

use crate::tracing::*;
use bitfield::{prelude::*, BitField};
use num_conv::prelude::*;

use crate::cpu::hart::{MStatus, WhiskerHart};
use crate::ty::{ExceptionBits, HartMode, RiscvExtensions, TrapIdx, TrapKind, TrapRequestGuaranteed};
use crate::util::extract_bits_16;

macro_rules! define_csrs {
    ($($name:ident, $addr:literal),*$(,)*) => {
		paste::paste! {$(
            #[allow(dead_code)]
			pub const [< $name:snake:upper >]: CSRIndex = CSRIndex($addr);
		)*}
	};
}

#[rustfmt::skip]
define_csrs!(
    // machine information registers
    mvendorid,  0xF11,
    marchid,    0xF12,
    mimpid,     0xF13,
    mhartid,    0xF14,
    mconfigptr, 0xF15,

    // machine trap setup
    // MIE set to 0, MPP set to M mode
    mstatus,    0x300,
    misa,       0x301,
    medeleg,    0x302,
    mideleg,    0x303,
    mie,        0x304,
    mtvec,      0x305,
    // 0x310 and 0x312 mstatush and medelegh are RV32 only

    // machine trap handling
    mscratch,   0x340,
    mepc,       0x341,
    mcause,     0x342,
    mtval,      0x343,
    mip,        0x344,
    // 0x34A and 0x34B mtinst and mtval2 are added by the hypervisor extension

    // ======================
    // S-mode CSRs
	// ======================
    sstatus,    0x100,
    sie,        0x104,
    stvec,      0x105,

    // supervisor trap handling
    sscratch,   0x140,
    sepc,       0x141,
    scause,     0x142,
    stval,      0x143,
    sip,        0x144,

    // supervisor protection and translation
    satp,       0x180,

    fcsr,       0x003,
);

const NUM_CSRS: u16 = 4096;

type CSRReadFn = fn(&mut WhiskerHart) -> u64;
type CSRWriteFn = fn(&mut WhiskerHart, val: u64);

macro_rules! constant {
	($val:expr) => {
		CSRInfo::new_read_only(|_| $val)
	};
}

macro_rules! read_write_trivial {
	($field:ident) => {
		CSRInfo::new_read_write(|hart| hart.$field, |hart, val| hart.$field = val)
	};
}

pub fn create_info() -> BTreeMap<CSRIndex, CSRInfo> {
	let mut reg_info = BTreeMap::default();
	reg_info.extend([
		// machine information registers
		// TODO: actually impl these maybe?
		(MVENDORID, constant!(0)),
		(MARCHID, constant!(0)),
		(MIMPID, constant!(0)),
		(MHARTID, CSRInfo::new_read_only(read_mhartid)),
		(MCONFIGPTR, constant!(0)),
		// machine trap setup
		(MSTATUS, CSRInfo::new_read_write(read_mstatus, write_mstatus)),
		(MISA, CSRInfo::new_read_write(read_misa, noop_writer)),
		(MEDELEG, CSRInfo::new_read_write(read_medeleg, write_medeleg)),
		(MIDELEG, CSRInfo::new_read_write(read_mideleg, write_mideleg)),
		(MIE, CSRInfo::new_read_write(read_mie, write_mie)),
		(MTVEC, CSRInfo::new_read_write(read_mtvec, write_mtvec)),
		// machine trap handling
		(MSCRATCH, read_write_trivial!(mscratch)),
		(MEPC, read_write_trivial!(mepc)),
		(MCAUSE, CSRInfo::new_read_write(read_mcause, write_mcause)),
		(MTVAL, read_write_trivial!(mtval)),
		(MIP, CSRInfo::new_read_write(read_mip, write_mip)),
		// supervisor trap setup
		(SSTATUS, CSRInfo::new_read_write(read_sstatus, write_sstatus)),
		(SIE, CSRInfo::new_read_write(read_sie, write_sie)),
		(STVEC, CSRInfo::new_read_write(read_stvec, write_stvec)),
		// supervisor trap handling
		(SSCRATCH, read_write_trivial!(sscratch)),
		(SEPC, read_write_trivial!(sepc)),
		(SCAUSE, CSRInfo::new_read_write(read_scause, write_scause)),
		(STVAL, read_write_trivial!(stval)),
		(SIP, CSRInfo::new_read_write(read_sip, write_sip)),
		// supervisor protection and translation
		(SATP, CSRInfo::new_read_write(read_satp, write_satp)),
	]);
	reg_info
}

// NOTE: this is on the hart not on the CSR struct because operations on CSRs may affect state
impl WhiskerHart {
	#[must_use]
	/// ensures that a CSR is readable. this means that it must exist and that the current mode of
	/// the hart is greater than or equal privilege level to the necessary privilege for the CSR.
	pub fn csr_require_ro(&mut self, idx: CSRIndex) -> Result<CSRReadToken, TrapRequestGuaranteed> {
		let info = self.csr_info.get(&idx);

		// FIXME: extension checks
		if info.is_some() && idx.required_mode() <= self.mode() {
			Ok(CSRReadToken { idx })
		} else {
			error!("ERROR: accessing {:?} ro", idx);

			Err(self.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0))
		}
	}

	#[must_use]
	/// ensures that a CSR is readable and writeable. this means that it must exist, that it must be
	/// writeable, and that the current mode of the hart is greater than or equal privilege level to
	/// the necessary privilege for the CSR.
	pub fn csr_require_rw(&mut self, idx: CSRIndex) -> Result<CSRReadWriteToken, TrapRequestGuaranteed> {
		let info = self.csr_info.get(&idx);

		// FIXME: extension checks
		if info.is_some_and(|i| i.is_rw()) && idx.required_mode() <= self.mode() {
			Ok(CSRReadWriteToken {
				inner: CSRReadToken { idx },
			})
		} else {
			error!("ERROR: accessing {:?} rw", idx);

			Err(self.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0))
		}
	}

	pub fn read_csr(&mut self, token: &CSRReadToken) -> u64 {
		let idx = token.idx;
		let read = match self.csr_info.get(&idx).unwrap().ops {
			CSROps::Read(read) => read,
			CSROps::ReadWrite(read, _) => read,
		};
		read(self)

		/*
		match idx {
			// MISA must always match the current cpu extension state
			MISA => self.read_misa(),
			MHARTID => self.hart_id().inner().extend(),

			// =========================================
			// supervisor level CSRs
			// most of these need to be restricted views
			// =========================================
			SSTATUS => self.read_sstatus(),

			// registers that need no special handling
			_ => self.csrs.0[idx.as_idx()].val,
		}
		*/
	}

	pub fn write_csr(&mut self, token: &CSRReadWriteToken, val: u64) {
		let idx = token.idx;
		let write = match self.csr_info.get(&idx).unwrap().ops {
			CSROps::Read(_) => unreachable!(),
			CSROps::ReadWrite(_, write) => write,
		};
		write(self, val);

		/*
		match idx {
			MSTATUS => self.write_mstatus(val),
			// we do not support modifying MISA so writes must be ignored
			MISA => (),
			MIE => self.write_mie(val),
			MIP => self.write_mip(val),
			// registers that need no special handling, or missing registers
			_ => self.csrs.0[idx.as_idx()].val = val,
		}
		*/
	}

	/// reads a csr without checks for existence or permissions
	/// this MUST only be used for implementing system control or status operations
	/// such as reading FCSR for float operations
	pub fn read_csr_unchecked(&mut self, _csr: CSRIndex) -> u64 {
		todo!("impl in new CSR system (probably just directly read fields)")
	}

	/// writes a csr without checks for existence or permissions
	/// this MUST only be used for implementing system control or status operations
	/// such as updating MCAUSE on traps
	pub fn write_csr_unchecked(&mut self, _csr: CSRIndex, _val: u64) {
		todo!("impl in new CSR system (probably just directly write fields)")
	}
}

// some CSRs want to ignore writes entirely
fn noop_writer(_hart: &mut WhiskerHart, _val: u64) {}

fn read_mhartid(hart: &mut WhiskerHart) -> u64 {
	hart.hart_id().inner().extend::<u64>()
}

fn read_mstatus(hart: &mut WhiskerHart) -> u64 {
	u64::from_le_bytes(hart.mstatus.inner())
}
fn write_mstatus(hart: &mut WhiskerHart, val: u64) {
	debug!("TODO: implement mstatus properly");
	hart.mstatus.set_inner(val.to_le_bytes());
}

fn read_misa(hart: &mut WhiskerHart) -> u64 {
	// read current hart extension state, with 64 bit MXLEN
	hart.supported_extensions().inner() | 0b10 << 62
}

fn read_medeleg(hart: &mut WhiskerHart) -> u64 {
	u64::from_le_bytes(hart.medeleg.inner())
}
fn write_medeleg(hart: &mut WhiskerHart, val: u64) {
	hart.medeleg.set_inner((val & ExceptionBits::MASK).to_le_bytes());
}

fn read_mideleg(hart: &mut WhiskerHart) -> u64 {
	u64::from_le_bytes(hart.mideleg.inner())
}
fn write_mideleg(hart: &mut WhiskerHart, val: u64) {
	hart.mideleg.set_inner((val & InterruptBits::MASK_M_MODE).to_le_bytes());
}

fn read_mie(hart: &mut WhiskerHart) -> u64 {
	u64::from_le_bytes(hart.mie.inner())
}
fn write_mie(hart: &mut WhiskerHart, val: u64) {
	hart.mie.set_inner((val & InterruptBits::MASK_M_MODE).to_le_bytes());
}

fn read_mtvec(hart: &mut WhiskerHart) -> u64 {
	u64::from_le_bytes(hart.mtvec.inner())
}
fn write_mtvec(hart: &mut WhiskerHart, val: u64) {
	debug_assert!(
		val & 0b11 == 0,
		"alternative mtvec modes not yet implemented or maybe you forgot __attribute__((aligned(4))) on a trap handler"
	);
	hart.mtvec.set_inner(val.to_le_bytes());
}

fn read_mcause(hart: &mut WhiskerHart) -> u64 {
	hart.mcause.inner()
}
fn write_mcause(hart: &mut WhiskerHart, val: u64) {
	hart.mcause = TrapIdx::from_raw(val)
}

fn read_mip(hart: &mut WhiskerHart) -> u64 {
	u64::from_le_bytes(hart.mip.inner())
}
fn write_mip(hart: &mut WhiskerHart, val: u64) {
	warn!(
		"writes to mip are ignored (hart {:?} wrote {:#018X})",
		hart.hart_id(),
		val
	);
}

fn read_sstatus(hart: &mut WhiskerHart) -> u64 {
	read_mstatus(hart) & MStatus::MASK_S_MODE
}
fn write_sstatus(hart: &mut WhiskerHart, val: u64) {
	trace!("hart {:?} write {:#018X} to sstatus", hart.hart_id(), val);
	let mstatus = read_mstatus(hart) & !MStatus::MASK_S_MODE;
	let val = val & MStatus::MASK_S_MODE;
	write_mstatus(hart, mstatus | val);
	trace!(
		"hart {:?} resulting mstatus: {:#018X}",
		hart.hart_id(),
		read_mstatus(hart)
	);
}

pub fn read_sie(hart: &mut WhiskerHart) -> u64 {
	read_mie(hart) & InterruptBits::MASK_S_MODE
}
fn write_sie(hart: &mut WhiskerHart, val: u64) {
	let mie = read_mie(hart) & !InterruptBits::MASK_S_MODE;
	let val = val & InterruptBits::MASK_S_MODE;
	write_mie(hart, mie | val);
}

fn read_stvec(hart: &mut WhiskerHart) -> u64 {
	u64::from_le_bytes(hart.stvec.inner())
}
fn write_stvec(hart: &mut WhiskerHart, val: u64) {
	debug_assert!(
		val & 0b11 == 0,
		"alternative stvec modes not yet implemented or maybe you forgot __attribute__((aligned(4))) on a trap handler"
	);
	hart.stvec.set_inner(val.to_le_bytes());
}

fn read_scause(hart: &mut WhiskerHart) -> u64 {
	hart.scause.inner()
}
fn write_scause(hart: &mut WhiskerHart, val: u64) {
	hart.scause = TrapIdx::from_raw(val);
}

pub fn read_sip(hart: &mut WhiskerHart) -> u64 {
	read_mip(hart) & InterruptBits::MASK_S_MODE
}
fn write_sip(hart: &mut WhiskerHart, val: u64) {
	warn!(
		"writes to sip are ignored (hart {:?} wrote {:#018X})",
		hart.hart_id(),
		val
	);
}

fn read_satp(hart: &mut WhiskerHart) -> u64 {
	u64::from_le_bytes(hart.translation_config.inner())
}
fn write_satp(hart: &mut WhiskerHart, val: u64) {
	let conf = AddressTranslationMode::from_bits((val >> 60).truncate());
	if !matches!(conf, AddressTranslationMode::Bare | AddressTranslationMode::Sv39) {
		unimplemented!("satp.MODE {:?} not supported", conf);
	}

	hart.translation_config.set_inner(val.to_le_bytes());
}

/// INVARIANT: holds a valid CSR index (0..NUM_CSRS)
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CSRIndex(u16);

impl CSRIndex {
	pub fn new(addr: u16) -> Option<Self> {
		(addr < NUM_CSRS).then_some(Self(addr))
	}

	pub fn required_mode(self) -> HartMode {
		let mode = extract_bits_16(self.0, 8, 9);
		HartMode::from_bits(mode.truncate())
	}
}

impl Debug for CSRIndex {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{:#05X}", self.0)
	}
}

#[derive(Debug)]
pub struct CSRInfo {
	required_extensions: RiscvExtensions,
	ops: CSROps,
}

#[derive(Debug)]
enum CSROps {
	Read(CSRReadFn),
	ReadWrite(CSRReadFn, CSRWriteFn),
}

impl CSRInfo {
	fn new_read_only(read: CSRReadFn) -> Self {
		Self {
			required_extensions: RiscvExtensions::empty(),
			ops: CSROps::Read(read),
		}
	}

	fn new_read_write(read: CSRReadFn, write: CSRWriteFn) -> Self {
		Self {
			required_extensions: RiscvExtensions::empty(),
			ops: CSROps::ReadWrite(read, write),
		}
	}

	fn is_rw(&self) -> bool {
		matches!(self.ops, CSROps::ReadWrite { .. })
	}
}

pub struct CSRReadToken {
	idx: CSRIndex,
}

pub struct CSRReadWriteToken {
	/// used so that a readwrite token can be used as a read token
	inner: CSRReadToken,
}

impl Deref for CSRReadWriteToken {
	type Target = CSRReadToken;

	fn deref(&self) -> &Self::Target {
		&self.inner
	}
}

macro_rules! assert_xlen {
	($ty:ty) => {
		assert!(::core::mem::size_of::<$ty>() == ::core::mem::size_of::<u64>());
	};
}

#[bitfields]
#[derive(Debug, Clone, Copy)]
pub struct InterruptBits {
	_res_0_0: U1,
	pub s_soft_interrupt: bool,
	_res_2_2: U1,
	pub m_soft_interrupt: bool,
	_res_4_4: U1,
	pub s_timer_interrupt: bool,
	_res_6_6: U1,
	pub m_timer_interrupt: bool,
	_res_8_8: U1,
	pub s_external_interrupt: bool,
	_res_10_10: U1,
	pub m_external_interrupt: bool,
	_res_12_12: U1,
	pub counter_overflow: bool,
	_res_14_63: U50,
}

impl InterruptBits {
	const MASK_M_MODE: u64 = 0b0010_1010_1010_1010;
	const MASK_S_MODE: u64 = 0b0010_0010_0010_0010;

	pub fn set_interrupt(&mut self, trap: TrapIdx, enabled: bool) {
		debug_assert!(trap.kind() == TrapKind::Interrupt);
		let mut inner = u64::from_le_bytes(self.inner());
		let mask = !((1 << trap.cause()) & Self::MASK_M_MODE);
		let bit = (u64::from(enabled) << trap.cause()) & Self::MASK_M_MODE;
		inner &= mask;
		inner |= bit;
		self.set_inner(inner.to_le_bytes());
	}

	pub fn is_enabled(self, trap: TrapIdx) -> bool {
		if trap.kind() != TrapKind::Interrupt {
			return false;
		}

		let inner = u64::from_le_bytes(self.inner());
		inner & (1 << trap.cause()) != 0
	}
}

#[bitfields]
#[derive(Debug, Clone, Copy)]
pub struct TrapVector {
	pub mode: U2,
	base: U62,
}

impl TrapVector {
	pub fn addr_for_trap(self, trap: TrapIdx) -> u64 {
		// FIXME: check mode of mtvec and handle offsets
		self.get_base() << 2
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, BitFieldRepr)]
#[allow(non_camel_case_types, reason = "reserved")]
pub enum AddressTranslationMode {
	Bare = 0,
	__reserved_1,
	__reserved_2,
	__reserved_3,
	__reserved_4,
	__reserved_5,
	__reserved_6,
	__reserved_7,
	Sv39,
	Sv48,
	Sv57,
	__reserved_Sv64,
	__reserved_12,
	__reserved_13,
	__reserved_14,
	__reserved_15,
}

#[bitfields]
#[derive(Debug, Clone, Copy)]
pub struct AddressTranslationConfig {
	pub root_page_num: U44,
	pub address_space_id: U16,
	pub mode: AddressTranslationMode,
}

const _: () = {
	assert_xlen!(InterruptBits);
	assert_xlen!(TrapVector);
	assert_xlen!(AddressTranslationConfig);
};
