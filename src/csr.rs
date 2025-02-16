use std::fmt::Debug;

struct CSRInfo {
	val: u64,
	addr: u16,
	rw: bool,
	/// 0 - user
	/// 1 - supervisor
	/// 2 - hypervisor
	/// 3 - machine
	privilege: u8,
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
					0 => "U",
					1 => "S",
					2 => "H",
					3 => "M",
					_ => unreachable!(),
				},
			)
			.finish()
	}
}

macro_rules! define_csrs {
    ($($name:ident, $addr:literal, $rw:ident, $priv:ident $(, $init:literal)?),*$(,)*) => {
		#[derive(Debug)]
		pub struct ControlStatusRegisters {
	    	$($name: CSRInfo),*
		}

		#[allow(unused)]
		impl ControlStatusRegisters {
    		pub fn new() -> Self {
    		    Self {
                    $(
    				$name: CSRInfo {
    				    val: {
                            let mut val = 0;
                            $( val = $init; )?
                            val
                        },
    				    addr: $addr,
    					rw: $rw,
    					privilege: $priv,
    				},
                    )*
    			}
    		}
		}

		paste::paste!{
		#[allow(unused)]
		impl ControlStatusRegisters {$(
		    pub const [< $name:snake:upper >]: u16 = $addr;

			pub fn [< read_ $name >](&self) -> u64 {
			    self.$name.val
			}

			pub fn [< write_ $name >](&mut self, val: u64) {
			    self.$name.val = val;
			}
		)*}
		}
	};
}

const RW: bool = true;
const RO: bool = false;
#[allow(unused)]
const UNPRIVILEGED: u8 = 0b00;
#[allow(unused)]
const SUPERVISOR: u8 = 0b01;
#[allow(unused)]
const HYPERVISOR: u8 = 0b10;
const MACHINE: u8 = 0b11;

#[rustfmt::skip]
define_csrs!(
    mvendorid, 0xF11, RO, MACHINE, 0,
    marchid,   0xF12, RO, MACHINE, 0,
    mimpid,    0xF13, RO, MACHINE, 0,

    mtvec,     0x305, RW, MACHINE, 0x4000_0000,
    mepc,      0x341, RW, MACHINE,
    mcause,    0x342, RW, MACHINE,
    mtval,     0x343, RW, MACHINE,
);
