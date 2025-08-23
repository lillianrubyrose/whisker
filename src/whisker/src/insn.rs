mod atomic;
mod compressed;
mod csr;
mod float;
mod int;
mod multiply;
mod privileged;

pub use atomic::AtomicInstruction;
pub use compressed::CompressedInstruction;
pub use csr::CSRInstruction;
pub use float::FloatInstruction;
pub use int::IntInstruction;
pub use multiply::MultiplyInstruction;
pub use privileged::PrivilegedInstruction;

#[derive(Debug, Clone)]
pub enum Instruction {
	IntExtension(IntInstruction),
	FloatExtension(FloatInstruction),
	Csr(CSRInstruction),
	CompressedExtension(CompressedInstruction),
	AtomicExtension(AtomicInstruction),
	MultiplyInstruction(MultiplyInstruction),
	PrivilegedInstruction(PrivilegedInstruction),
}
