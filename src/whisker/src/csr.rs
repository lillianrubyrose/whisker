use std::collections::HashMap;
use std::fmt::Debug;

pub const NUM_CSRS: u16 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
#[allow(dead_code)]
pub enum CSRPrivilege {
	User = 0b00,
	Supervisor = 0b01,
	Hypervisor = 0b10,
	Machine = 0b11,
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

macro_rules! define_csrs {
    ($($name:ident, $addr:literal, $rw:ident, $priv:ident $(, $init:literal)?),*$(,)*) => {
		#[derive(Debug)]
		pub struct ControlStatusRegisters {
		    regs: HashMap<u16, CSRInfo>,
		}

		#[allow(unused)]
		impl ControlStatusRegisters {
    		pub fn new() -> Self {
    		    Self {
                    regs: {
                        let mut map = HashMap::new();
                        $(map.insert(
                            $addr,
                            CSRInfo {
                                val: {
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

		paste::paste!{
		#[allow(unused)]
		impl ControlStatusRegisters {$(
		    pub const [< $name:snake:upper >]: u16 = $addr;

			pub fn [< read_ $name >](&self) -> u64 {
			    self.regs.get(&$addr).unwrap().val
			}

			pub fn [< write_ $name >](&mut self, val: u64) {
			    self.regs.get_mut(&$addr).unwrap().val = val;
			}
		)*}
		}
	};
}

impl ControlStatusRegisters {
	pub fn get(&self, reg: u16) -> Option<&CSRInfo> {
		assert!(reg < NUM_CSRS);
		self.regs.get(&reg)
	}

	pub fn get_mut(&mut self, reg: u16) -> Option<&mut CSRInfo> {
		assert!(reg < NUM_CSRS);
		self.regs.get_mut(&reg)
	}
}

const RW: bool = true;
const RO: bool = false;

#[rustfmt::skip]
define_csrs!(
    mvendorid, 0xF11, RO, Machine, 0,
    marchid,   0xF12, RO, Machine, 0,
    mimpid,    0xF13, RO, Machine, 0,

    mtvec,     0x305, RW, Machine, 0x4000_0000,
    mepc,      0x341, RW, Machine,
    mcause,    0x342, RW, Machine,
    mtval,     0x343, RW, Machine,

    fcsr,      0x003, RW, User,
);
