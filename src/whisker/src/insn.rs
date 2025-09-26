mod atomic;
mod compressed;
mod csr;
mod double;
mod float;
mod int;
mod multiply;
mod privileged;

pub use atomic::AtomicInstruction;
pub use compressed::CompressedInstruction;
pub use csr::CSRInstruction;
pub use double::DoubleInstruction;
pub use float::FloatInstruction;
pub use int::IntInstruction;
pub use multiply::MultiplyInstruction;
pub use privileged::PrivilegedInstruction;

#[derive(Debug, Clone, Copy)]
pub enum Instruction {
	Int(IntInstruction),
	Float(FloatInstruction),
	Double(DoubleInstruction),
	Zicsr(CSRInstruction),
	Compressed(CompressedInstruction),
	Atomic(AtomicInstruction),
	Multipliy(MultiplyInstruction),
	Privileged(PrivilegedInstruction),
}
