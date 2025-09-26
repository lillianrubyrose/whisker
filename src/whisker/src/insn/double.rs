use crate::insn::Instruction;
use crate::ty::{FPRegisterIndex, GPRegisterIndex};

#[derive(Debug, Clone, Copy)]
pub enum DoubleInstruction {
    Store {
        dst: GPRegisterIndex,
        dst_offset: i64,
        src: FPRegisterIndex,
    }
}

impl From<DoubleInstruction> for Instruction {
    fn from(val: DoubleInstruction) -> Self {
        Instruction::Double(val)
    }
}
