use std::collections::BTreeMap;
use std::fmt::Debug;
use std::ops::Deref;

use crate::tracing::*;
use bitfield::{prelude::*, BitField};
use num_conv::prelude::*;

use crate::cpu::hart::{MStatus, WhiskerHart};
use crate::ty::{ExceptionBits, HartMode, RiscvExtensions, TrapIdx, TrapKind, TrapRequestGuaranteed};
use crate::util::extract_bits_16;

const NUM_CSRS: u16 = 4096;

type CSRReadFn = fn(&mut WhiskerHart) -> u64;
type CSRWriteFn = fn(&mut WhiskerHart, val: u64);

macro_rules! read_write_trivial {
	($field:ident) => {
		(
			(|hart: &mut WhiskerHart| hart.$field) as CSRReadFn,
			(|hart: &mut WhiskerHart, val: u64| hart.$field = val) as CSRWriteFn,
		)
	};
}

macro_rules! impl_csrs {
	() => {};
	($reg_info:expr, $name:ident, $addr:expr, rw $readwrite:expr; $($tail:tt)*) => {{
		let (read, write): (CSRReadFn, CSRWriteFn) = $readwrite;
		($reg_info).insert(
			CSRIndex($addr), CSRInfo::new_read_write(stringify!($name), read, write)
		);
		impl_csrs!($($tail)*);
	}};
	($reg_info:expr, $name:ident, $addr:expr, const $val:literal; $($tail:tt)*) => {{
		($reg_info).insert(
			CSRIndex($addr), CSRInfo::new_read_only(stringify!($name), |_| $val),
		);
		impl_csrs!($($tail)*);
	}};
	($reg_info:expr, $name:ident, $addr:expr, ro $read:expr; $($tail:tt)*) => {{
		($reg_info).insert(
			CSRIndex($addr), CSRInfo::new_read_only(stringify!($name), $read),
		);
		impl_csrs!($($tail)*);
	}};
}

macro_rules! define_pmp_cfg_regs {
	($reg_info:expr, $($id:literal)*) => {
		paste::paste!{$(
			const _: () = assert!($id % 2 == 0, "RV64 only uses pmpcfg 0,2,...");
		)*}
		paste::paste!{
			($reg_info).extend([$(
				(CSRIndex({0x3A0 + $id}), CSRInfo::new_read_write(stringify!([< pmpcfg $id:snake:lower >]), |_| 0, |_, _| {})),
			)*]);
		}
	};
}

macro_rules! define_pmp_addr_regs {
	($reg_info:expr, $($id:literal)*) => {
		paste::paste!{
			($reg_info).extend([$(
				(CSRIndex({0x3B0 + $id}), CSRInfo::new_read_write(stringify!([< pmpaddr $id:snake:lower >]), |_| 0, |_, _| {})),
			)*]);
		}
	};
}

pub fn create_info() -> BTreeMap<CSRIndex, CSRInfo> {
	let mut csrs = BTreeMap::default();
	#[rustfmt::skip]
	impl_csrs!(
		// machine information registers
		// TODO: actually impl these maybe?
		csrs, mvendorid,  0xF11, const 0;
		csrs, marchid,    0xF12, const 0;
		csrs, mimpid,     0xF13, const 0;
		csrs, mhartid,    0xF14, ro read_mhartid;
		csrs, mconfigptr, 0xF15, const 0;

		// machine trap setup
		csrs, mstatus, 0x300, rw (read_mstatus, write_mstatus);
		csrs, misa,    0x301, rw (read_misa, noop_writer);
		csrs, medeleg, 0x302, rw (read_medeleg, write_medeleg);
		csrs, mideleg, 0x303, rw (read_mideleg, write_mideleg);
		csrs, mie,     0x304, rw (read_mie, write_mie);
		csrs, mtvec,   0x305, rw (read_mtvec, write_mtvec);

		// machine trap handling
		csrs, mscratch, 0x340, rw read_write_trivial!(mscratch);
		csrs, mepc,     0x341, rw read_write_trivial!(mepc);

		csrs, mcause,   0x342, rw (read_mcause, write_mcause);
		csrs, mtval,    0x343, rw read_write_trivial!(mtval);
		csrs, mip,      0x344, rw (read_mip, write_mip);

		// supervisor trap setup
		csrs, sstatus, 0x100, rw (read_sstatus, write_sstatus);
		csrs, sie,     0x104, rw (read_sie, write_sie);
		csrs, stvec,   0x105, rw (read_stvec, write_stvec);

		// supervisor trap handling
		csrs, sscratch, 0x140, rw read_write_trivial!(sscratch);
		csrs, sepc,     0x141, rw read_write_trivial!(sepc);
		csrs, scause,   0x142, rw (read_scause, write_scause);
		csrs, stval,    0x143, rw read_write_trivial!(stval);
		csrs, sip,      0x144, rw (read_sip, write_sip);

		// supervisor protection and translation
		csrs, satp, 0x180, rw (read_satp, write_satp);

		// float status
		csrs, fflags, 0x001, rw (
									|hart| read_fcsr(hart) & 0b11111,
									|hart, val| write_fcsr(hart, val & 0b11111)
								);
		csrs, frm,    0x002, rw (|hart| read_fcsr(hart) >> 5, |hart, val| write_fcsr(hart, val >> 5));
		csrs, fcsr,   0x003, rw (read_fcsr, write_fcsr);
	);

	define_pmp_cfg_regs!(&mut csrs, 0 2 4 6 8 10 12 14);
	define_pmp_addr_regs!(&mut csrs, 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37 38 39 40 41 42 43 44 45 46 47 48 49 50 51 52 53 54 55 56 57 58 59 60 61 62 63);

	csrs
}

pub fn generate_csr_xml(info: &BTreeMap<CSRIndex, CSRInfo>) -> String {
	let mut xml = String::from("  <feature name=\"org.gnu.gdb.riscv.csr\">\n");
	const GDB_CSR_BASE: u16 = 65;
	for (idx, info) in info.iter() {
		xml.push_str(&format!(
			"    <reg name=\"{}\" bitsize=\"64\" regnum=\"{}\"/>\n",
			info.name,
			idx.0 + GDB_CSR_BASE,
		));
	}
	xml.push_str("  </feature>\n");
	xml
}

// NOTE: this is on the hart not on the CSR struct because operations on CSRs may affect state
impl WhiskerHart {
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
	}

	pub fn write_csr(&mut self, token: &CSRReadWriteToken, val: u64) {
		let idx = token.idx;
		let write = match self.csr_info.get(&idx).unwrap().ops {
			CSROps::Read(_) => unreachable!(),
			CSROps::ReadWrite(_, write) => write,
		};
		write(self, val);
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
	hart.mcause = TrapIdx::from_raw(val);
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

fn read_fcsr(hart: &mut WhiskerHart) -> u64 {
	u8::from_le_bytes(hart.float_status_control.inner()).extend::<u64>()
}
fn write_fcsr(hart: &mut WhiskerHart, val: u64) {
	let val = val.truncate::<u8>();
	hart.float_status_control.set_inner([val]);
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
	#[allow(dead_code, reason = "FIXME: We should be using this")]
	required_extensions: RiscvExtensions,
	ops: CSROps,
	name: &'static str,
}

#[derive(Debug)]
enum CSROps {
	Read(CSRReadFn),
	ReadWrite(CSRReadFn, CSRWriteFn),
}

impl CSRInfo {
	fn new_read_only(name: &'static str, read: CSRReadFn) -> Self {
		Self {
			required_extensions: RiscvExtensions::empty(),
			ops: CSROps::Read(read),
			name,
		}
	}

	fn new_read_write(name: &'static str, read: CSRReadFn, write: CSRWriteFn) -> Self {
		Self {
			required_extensions: RiscvExtensions::empty(),
			ops: CSROps::ReadWrite(read, write),
			name,
		}
	}

	fn is_rw(&self) -> bool {
		matches!(self.ops, CSROps::ReadWrite { .. })
	}
}

#[must_use]
pub struct CSRReadToken {
	idx: CSRIndex,
}

#[must_use]
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
	pub fn addr_for_trap(self, _trap: TrapIdx) -> u64 {
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
