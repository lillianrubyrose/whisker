use std::collections::HashMap;
use std::fmt::Debug;
use std::ops::Deref;

use crate::cpu::WhiskerCpu;
use crate::ty::TrapIdx;

macro_rules! define_csrs {
    ($($name:ident, $addr:literal, $rw:ident, $priv:ident $(, $init:literal)?),*$(,)*) => {
		paste::paste! {
			$(
            #[allow(dead_code)]
			pub const [< $name:snake:upper >]: CSRIndex = CSRIndex($addr);
			)*

			impl ControlStatusRegisters {
    			pub fn new() -> Self {
          		    Self {
                            regs: {
                                let mut map = HashMap::new();
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
    mvendorid, 0xF11, RO, Machine, 0,
    marchid,   0xF12, RO, Machine, 0,
    mimpid,    0xF13, RO, Machine, 0,
    mhartid,   0xF14, RO, Machine, 0, // we only support hart0

    // machine trap setup
    mtvec,     0x305, RW, Machine, 0,

    // machine trap handling
    mepc,      0x341, RW, Machine,
    mcause,    0x342, RW, Machine,
    mtval,     0x343, RW, Machine,

    fcsr,      0x003, RW, User,
);

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
	regs: HashMap<CSRIndex, CSRInfo>,
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
			// registers that need no special handling, or missing registers
			_ => match self.csrs.regs.get_mut(&idx) {
				Some(info) => {
					info.val = val;
				}
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
