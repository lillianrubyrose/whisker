use std::{collections::BTreeMap, fmt::Debug, ops::Deref};

use bitfield::{BitField, prelude::*};
use bytemuck::bytes_of_mut;
use num_conv::prelude::*;

use crate::{
	cpu::hart::{MStatus, WhiskerHart},
	mem::mmio::MMIOKind,
	tracing::*,
	ty::{ExceptionBits, HartMode, RiscvExtensions, TrapIdx, TrapKind, TrapRequestGuaranteed},
	util::extract_bits_16,
};

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

macro_rules! define_pmp_addr_accessors {
    ($($id:literal),*) => {
        paste::paste! {
            $(
                fn [<read_pmpaddr_ $id>](hart: &mut WhiskerHart) -> u64 {
                    hart.pmpaddr[$id]
                }
                fn [<write_pmpaddr_ $id>](hart: &mut WhiskerHart, val: u64) {
                    hart.pmpaddr[$id] = val;
                }
            )*
        }
    };
}

macro_rules! register_pmp_addrs {
    ($reg_info:expr, $($id:literal),*) => {
        paste::paste! {
            $(
                ($reg_info).insert(
                    CSRIndex(0x3B0 + $id),
                    CSRInfo::new_read_write(
                        stringify!([< pmpaddr $id >]),
                        [<read_pmpaddr_ $id>],
                        [<write_pmpaddr_ $id>]
                    )
                );
            )*
        }
    };
}

define_pmp_addr_accessors!(
	0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30,
	31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59,
	60, 61, 62, 63
);

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

		csrs, pmpcfg0, 0x3A0, rw (read_pmpcfg0, write_pmpcfg0);
		csrs, pmpcfg2, 0x3A2, rw (read_pmpcfg2, write_pmpcfg2);

		csrs, mcounteren, 0x306, rw read_write_trivial!(mcounteren);
		csrs, scounteren, 0x106, rw read_write_trivial!(scounteren);

		// float status
		csrs, fflags, 0x001, rw (
									|hart| read_fcsr(hart) & 0b11111,
									|hart, val| {
									     let pre = read_fcsr(hart);
									     write_fcsr(hart, (pre & !0b11111) | (val & 0b11111));
									}
								);
		csrs, frm,    0x002, rw (|hart| read_fcsr(hart) >> 5, |hart, val| {
			let pre = read_fcsr(hart);
			write_fcsr(hart, (pre & !0b11100000) | ((val & 0b111) << 5));
		});
		csrs, fcsr,   0x003, rw (read_fcsr, write_fcsr);

		csrs, time, 0xc01, ro read_time;

        csrs, tselect,  0x7A0, rw (read_tselect, write_tselect);
        csrs, tdata1,   0x7A1, rw (read_tdata1, write_tdata1);
        csrs, tdata2,   0x7A2, rw (read_tdata2, write_tdata2);
        csrs, tcontrol, 0x7A5, rw (read_tcontrol, write_tcontrol);

        csrs, cycle,    0xC00, ro read_mcycle;
        csrs, instret,  0xC02, ro read_minstret;

        csrs, mcycle,   0xB00, rw (read_mcycle, write_mcycle);
        csrs, minstret, 0xB02, rw (read_minstret, write_minstret);

        csrs, menvcfg,  0x30A, rw (read_menvcfg, write_menvcfg);
	);

	register_pmp_addrs!(
		&mut csrs, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26,
		27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54,
		55, 56, 57, 58, 59, 60, 61, 62, 63
	);

	csrs
}

pub fn generate_csr_xml(info: &BTreeMap<CSRIndex, CSRInfo>) -> String {
	use std::fmt::Write;

	const GDB_CSR_BASE: u16 = 65;
	let mut xml = String::from("  <feature name=\"org.gnu.gdb.riscv.csr\">\n");
	for (idx, info) in info.iter() {
		let _ = write!(
			xml,
			"    <reg name=\"{}\" bitsize=\"64\" regnum=\"{}\"/>",
			info.name,
			idx.0 + GDB_CSR_BASE,
		);
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
			error!("ERROR: accessing {:?} ro mode {:?}", idx, self.mode());

			Err(self.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0))
		}
	}

	/// ensures that a CSR is readable and writeable. this means that it must exist, that it must be
	/// writeable, and that the current mode of the hart is greater than or equal privilege level to
	/// the necessary privilege for the CSR.
	pub fn csr_require_rw(&mut self, idx: CSRIndex) -> Result<CSRReadWriteToken, TrapRequestGuaranteed> {
		let info = self.csr_info.get(&idx);

		// FIXME: extension checks
		if info.is_some_and(CSRInfo::is_rw) && idx.required_mode() <= self.mode() {
			Ok(CSRReadWriteToken {
				inner: CSRReadToken { idx },
			})
		} else {
			error!("ERROR: accessing {:?} rw mode {:?}", idx, self.mode());

			Err(self.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0))
		}
	}

	pub fn read_csr(&mut self, token: &CSRReadToken) -> u64 {
		let idx = token.idx;
		let read = match self.csr_info.get(&idx).unwrap().ops {
			CSROps::ReadWrite(read, _) | CSROps::Read(read) => read,
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
	let mut new_mstatus = MStatus::new();
	new_mstatus.set_inner(val.to_le_bytes());

	// Chapter 2.3 defines WARL and such fields
	// Chapter 3.1.6.3 says that SXL and UXL are WARL fields
	// VS are defined as WARL in Chapter 3.1.6.7
	// SD is defined as read-only in Chapter 3.1.6.7

	hart.mstatus.set_sie(new_mstatus.get_sie());
	hart.mstatus.set_mie(new_mstatus.get_mie());
	hart.mstatus.set_spie(new_mstatus.get_spie());
	hart.mstatus.set_mpie(new_mstatus.get_mpie());
	hart.mstatus.set_spp(new_mstatus.get_spp());
	hart.mstatus.set_mpp(new_mstatus.get_mpp());
	hart.mstatus.set_mprv(new_mstatus.get_mprv());
	hart.mstatus.set_sum(new_mstatus.get_sum());
	hart.mstatus.set_mxr(new_mstatus.get_mxr());
	hart.mstatus.set_tvm(new_mstatus.get_tvm());

	// If F extension is supported then FS is writable
	if hart.supports_extensions(RiscvExtensions::FLOAT) {
		hart.mstatus.set_fs(new_mstatus.get_fs());
	}
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
	hart.mip.set_inner(val.to_le_bytes());
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
	let mie = read_mip(hart) & !InterruptBits::MASK_S_MODE;
	let val = val & InterruptBits::MASK_S_MODE;
	write_mip(hart, mie | val);
}

fn read_satp(hart: &mut WhiskerHart) -> u64 {
	u64::from_le_bytes(hart.translation_config.inner())
}
fn write_satp(hart: &mut WhiskerHart, val: u64) {
	let conf = AddressTranslationMode::from_bits((val >> 60).truncate());
	if !matches!(
		conf,
		AddressTranslationMode::Bare | AddressTranslationMode::Sv39 | AddressTranslationMode::Sv57
	) {
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
fn read_time(hart: &mut WhiskerHart) -> u64 {
	let mut val = 0_u64;
	let buf = bytes_of_mut(&mut val);
	MMIOKind::Clint.read(hart, crate::mem::mmio::clint::MTIME, buf);
	val
}

fn read_pmpcfg0(hart: &mut WhiskerHart) -> u64 {
	hart.pmpcfg[0]
}
fn write_pmpcfg0(hart: &mut WhiskerHart, val: u64) {
	hart.pmpcfg[0] = val;
}
fn read_pmpcfg2(hart: &mut WhiskerHart) -> u64 {
	hart.pmpcfg[1]
}
fn write_pmpcfg2(hart: &mut WhiskerHart, val: u64) {
	hart.pmpcfg[1] = val;
}

fn read_tselect(hart: &mut WhiskerHart) -> u64 {
	hart.tselect
}
fn write_tselect(hart: &mut WhiskerHart, val: u64) {
	hart.tselect = val;
}

fn read_tcontrol(hart: &mut WhiskerHart) -> u64 {
	u64::from_le_bytes(hart.tcontrol.inner())
}
fn write_tcontrol(hart: &mut WhiskerHart, val: u64) {
	hart.tcontrol.set_inner(val.to_le_bytes());
}
fn read_tdata1(hart: &mut WhiskerHart) -> u64 {
	let index = hart.tselect as usize;
	if index < hart.debug_triggers.len() {
		u64::from_le_bytes(hart.debug_triggers[index].0.inner())
	} else {
		0
	}
}
fn write_tdata1(hart: &mut WhiskerHart, val: u64) {
	let index = hart.tselect as usize;
	if index < hart.debug_triggers.len() {
		hart.debug_triggers[index].0.set_inner(val.to_le_bytes());
	}
}
fn read_tdata2(hart: &mut WhiskerHart) -> u64 {
	let index = hart.tselect as usize;
	if index < hart.debug_triggers.len() {
		hart.debug_triggers[index].1
	} else {
		0
	}
}
fn write_tdata2(hart: &mut WhiskerHart, val: u64) {
	let index = hart.tselect as usize;
	if index < hart.debug_triggers.len() {
		hart.debug_triggers[index].1 = val;
	}
}

fn read_mcycle(hart: &mut WhiskerHart) -> u64 {
	hart.cycles
}
fn write_mcycle(hart: &mut WhiskerHart, val: u64) {
	hart.cycles = val;
	hart.suppress_instret_increment = true;
}

fn read_minstret(hart: &mut WhiskerHart) -> u64 {
	hart.minstret
}
fn write_minstret(hart: &mut WhiskerHart, val: u64) {
	hart.minstret = val;
	hart.suppress_instret_increment = true;
}

fn read_menvcfg(hart: &mut WhiskerHart) -> u64 {
	u64::from_le_bytes(hart.menvcfg.inner())
}
fn write_menvcfg(hart: &mut WhiskerHart, val: u64) {
	// FIXME: If Svadu extension is not enabled then ADEU bit is a read-only zero bit
	// Right now we're just acting like Svadu is always enabled, idk if we should deal with
	// the case of having it disable-able? Grgrgr. I love you Citrine <3
	hart.menvcfg.set_inner(val.to_le_bytes());
}

/// INVARIANT: holds a valid CSR index (0..`NUM_CSRS`)
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

#[macro_export]
macro_rules! assert_xlen {
    ($($ty:ty),+) => {
        $(
            assert!(::core::mem::size_of::<$ty>() == ::core::mem::size_of::<u64>());
        )+
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
	pub fn addr_for_trap(self, trap: TrapIdx) -> u64 {
		let base = self.get_base() << 2;
		let mode = self.get_mode();

		// Section 3.1
		// 0 = Direct. All traps set pc to BASE.
		// 1 = Vectored. Asynchronous interrupts set pc to BASE+4*cause
		if mode == 1 && trap.kind() == TrapKind::Interrupt {
			base.wrapping_add(trap.cause() * 4)
		} else {
			base
		}
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
