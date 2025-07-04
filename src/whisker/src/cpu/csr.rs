use std::fmt::Debug;
use std::ops::Deref;

use rustc_hash::FxHashMap;
use tracing::error;

use crate::cpu::WhiskerCpu;
use crate::ty::TrapIdx;

macro_rules! define_csrs {
    ($($name:ident, $addr:literal, $rw:ident, $priv:ident $(= $init:expr)? ),*$(,)*) => {
		paste::paste! {
			$(
            #[allow(dead_code)]
			pub const [< $name:snake:upper >]: CSRIndex = CSRIndex($addr);
			)*

			impl ControlStatusRegisters {
    			pub fn new() -> Self {
          		    Self {
                            regs: {
                                let mut map = FxHashMap::default();
                                $(map.insert(
                                    CSRIndex($addr),
                                    CSRInfo {
                                        val: {
                                            // some cases dont expand to have the = init case, so ignore those warnings
                                            #[allow(unused)]
                                            let mut val = 0;
                                            $( val = $init; )?
                                            val
                                        },
                                        addr: $addr,
                                        rw: $rw,
                                        privilege: CSRPrivilege::$priv,
                                    },
                                );)*
                                map
                            }
         			}
        		}
			}
		}
	};
}

const RW: bool = true;
const RO: bool = false;

#[rustfmt::skip]
define_csrs!(
    mvendorid, 0xF11, RO, Machine = 0,
    marchid,   0xF12, RO, Machine = 0,
    mimpid,    0xF13, RO, Machine = 0,
    mhartid,   0xF14, RO, Machine = 0, // we only support hart0

    // machine trap setup
    // MIE set to 0, MPP set to M mode
    mstatus,   0x300, RW, Machine = 0b11 << 11,
    misa,      0x301, RW, Machine,
    // 0x302 and 0x303 MEDELEG and MIDELEG should not exist because S-mode is not implemented
    mie,       0x304, RW, Machine = 0, // by default all interrupt causes are disabled
    mtvec,     0x305, RW, Machine = 0,

    // machine trap handling
    mepc,      0x341, RW, Machine,
    mcause,    0x342, RW, Machine,
    mtval,     0x343, RW, Machine,
    mip,       0x344, RW, Machine,

    fcsr,      0x003, RW, User,
);

pub mod mstatus {
	pub const MIE_BIT: u8 = 3;
	pub const MPIE_BIT: u8 = 7;
	pub const MPP_START: u8 = 11;
	pub const MPP_END: u8 = 12;

	pub const MIE: u64 = 1 << 3;
	pub const MPIE: u64 = 1 << 7;
}

const NUM_CSRS: u16 = 4096;

/// INVARIANT: holds a valid CSR index (0..NUM_CSRS)
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CSRIndex(u16);

impl CSRIndex {
	pub fn new(addr: u16) -> Option<Self> {
		(addr < NUM_CSRS).then_some(Self(addr))
	}
}

impl Debug for CSRIndex {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{:#05X}", self.0)
	}
}

#[derive(Debug)]
pub struct ControlStatusRegisters {
	regs: FxHashMap<CSRIndex, CSRInfo>,
}

// NOTE: this is on the CPU not CSRs because operations on CSRs may affect cpu state
impl WhiskerCpu {
	#[must_use]
	pub fn csr_require_ro(&mut self, idx: CSRIndex) -> Option<CSRReadToken> {
		// all csrs that exist are considered readable
		if self.csrs.regs.get(&idx).is_some() {
			Some(CSRReadToken { idx })
		} else {
			self.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
			None
		}
	}

	#[must_use]
	pub fn csr_require_rw(&mut self, idx: CSRIndex) -> Option<CSRReadWriteToken> {
		if self.csrs.regs.get(&idx).is_some_and(|info| info.is_rw()) {
			Some(CSRReadWriteToken {
				inner: CSRReadToken { idx },
			})
		} else {
			self.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
			None
		}
	}

	pub fn read_csr(&mut self, token: &CSRReadToken) -> u64 {
		let idx = token.idx;
		match idx {
			// MISA must always match the current cpu extension state
			MISA => self.read_misa(),
			// registers that need no special handling
			_ => match self.csrs.regs.get(&idx) {
				Some(info) => info.val,
				// the token ensures the CSR exists
				None => unreachable!(),
			},
		}
	}

	pub fn write_csr(&mut self, token: &CSRReadWriteToken, val: u64) {
		let idx = token.idx;
		match idx {
			MSTATUS => self.write_mstatus(val),
			// we do not support modifying MISA so writes must be ignored
			MISA => (),
			MIE => self.write_mie(val),
			MIP => self.write_mip(val),
			// registers that need no special handling, or missing registers
			_ => match self.csrs.regs.get_mut(&idx) {
				Some(info) => info.val = val,
				// the token ensures that the CSR exists
				None => unreachable!(),
			},
		}
	}

	/// reads a csr without checks for existence or permissions
	/// this MUST only be used for implementing system control or status operations
	/// such as reading FCSR for float operations
	#[cfg_attr(debug_assertions, track_caller)] // provides better panic location info on misuse
	pub fn read_csr_unchecked(&mut self, csr: CSRIndex) -> u64 {
		self.csrs.regs.get(&csr).unwrap().val
	}

	/// writes a csr without checks for existence or permissions
	/// this MUST only be used for implementing system control or status operations
	/// such as updating MCAUSE on traps
	#[cfg_attr(debug_assertions, track_caller)] // provides better panic location info on misuse
	pub fn write_csr_unchecked(&mut self, csr: CSRIndex, val: u64) {
		self.csrs.regs.get_mut(&csr).unwrap().val = val;
	}
}

// special CSRs that need to ignore fields or do other non-trivial logic for reads
impl WhiskerCpu {
	fn read_misa(&self) -> u64 {
		// 64 bit XLEN
		2 << 62 | self.supported_extensions.inner()
	}
}

// special CSRs that need to ignore fields or do other non-trivial logic for writes
impl WhiskerCpu {
	fn write_mstatus(&mut self, val: u64) {
		// we only implement MIE, MPIE, and MPP
		// all other bits are read-only 0
		// however MPP is read-only 0b11
		// FIXME: VS, FS, XS, SD?
		const MSTATUS_WRITE_MASK: u64 = 1 << 3 | 1 << 7;
		error!("not yet implemented: side effects for MSTATUS");
		let other = self.read_csr_unchecked(MSTATUS) & !MSTATUS_WRITE_MASK;
		let val = val & MSTATUS_WRITE_MASK;
		self.write_csr_unchecked(MSTATUS, other | val);
		// FIXME: should we always do this check or only when MIE is changed?
		self.check_interrupt_trap();
	}

	fn write_mie(&mut self, val: u64) {
		// machine software, machine timer, and machine external interrupts
		const INTERRUPT_ENABLE_BITS: u64 = 1 << 3 | 1 << 7 | 1 << 11;
		let other = self.read_csr_unchecked(MIE) & !INTERRUPT_ENABLE_BITS;
		let val = val & INTERRUPT_ENABLE_BITS;
		self.write_csr_unchecked(MIE, other | val);
		self.check_interrupt_trap();
	}

	fn write_mip(&mut self, val: u64) {
		// machine software, machine timer, and machine external interrupts all use other mechanisms
		// to become pending.
		// this function therefore does not write to MIP, but exists so that future implemented
		// interrupts might be able to use it.
		// FIXME(csr): implement the above comment correctly; for the moment this allows all writes
		const INTERRUPT_PENDING_BITS: u64 = u64::MAX;
		let other = self.read_csr_unchecked(MIP) & !INTERRUPT_PENDING_BITS;
		let val = val & INTERRUPT_PENDING_BITS;
		self.write_csr_unchecked(MIP, other | val);
		self.check_interrupt_trap();
	}
}

pub struct CSRInfo {
	pub val: u64,
	addr: u16,
	rw: bool,
	privilege: CSRPrivilege,
}

#[allow(unused)]
impl CSRInfo {
	#[inline]
	pub fn addr(&self) -> u16 {
		self.addr
	}
	#[inline]
	pub fn is_rw(&self) -> bool {
		self.rw
	}
	#[inline]
	pub fn privilege(&self) -> CSRPrivilege {
		self.privilege
	}
}

impl Debug for CSRInfo {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("CSRInfo")
			.field("val", &format_args!("{:#018X}", self.val))
			.field("addr", &format_args!("{:#06X}", self.addr))
			.field("rw", if self.rw { &"RW" } else { &"RO" })
			.field(
				"privilege",
				&match self.privilege {
					CSRPrivilege::User => "U",
					CSRPrivilege::Supervisor => "S",
					CSRPrivilege::Hypervisor => "H",
					CSRPrivilege::Machine => "M",
				},
			)
			.finish()
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum CSRPrivilege {
	User = 0b00,
	#[expect(unused, reason = "S mode not implemented")]
	Supervisor = 0b01,
	#[expect(unused, reason = "H mode not implemented")]
	Hypervisor = 0b10,
	Machine = 0b11,
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
