use crate::cpu::hart::WhiskerHart;
use crate::{insn::*, insn32::BType};

macro_rules! parse_branch {
	($btype:ident, $($const:ident, $inst:ident),*) => {{
		use consts::*;
		match $btype.func() {
			$( $const => Some(IntInstruction::$inst { lhs: $btype.src1().to_gp(), rhs: $btype.src2().to_gp(), imm: $btype.imm() }.into()), )*
			_ => None,
		}
	}};
}

#[rustfmt::skip]
pub fn parse_branch(_: &mut WhiskerHart, parcel: u32) -> Option<Instruction> {
	let btype = BType::parse(parcel);
	parse_branch!(
		btype,
		BRANCH_EQ, BranchEqual,
		BRANCH_NEQ, BranchNotEqual,
		BRANCH_LESS_THAN, BranchLessThan,
		BRANCH_GREATER_EQ, BranchGreaterEqual,
		BRANCH_LESS_THAN_UNSIGNED, BranchLessThanUnsigned,
		BRANCH_GREATER_EQ_UNSIGNED, BranchGreaterEqualUnsigned
	)
}

pub mod consts {
	pub const BRANCH_EQ: u8 = 0b000;
	pub const BRANCH_NEQ: u8 = 0b001;
	pub const BRANCH_LESS_THAN: u8 = 0b100;
	pub const BRANCH_GREATER_EQ: u8 = 0b101;
	pub const BRANCH_LESS_THAN_UNSIGNED: u8 = 0b110;
	pub const BRANCH_GREATER_EQ_UNSIGNED: u8 = 0b111;
}
