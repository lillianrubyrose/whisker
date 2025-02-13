#[cfg(not(target_pointer_width = "64"))]
compile_error!("whisker only supports 64bit architectures");

use std::ops::{BitAnd, BitOr, BitOrAssign, BitAndAssign, Not};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupportedExtensions(u64);

impl SupportedExtensions {
    pub const INTEGER: SupportedExtensions = SupportedExtensions(0b1);

    pub const fn empty() -> Self {
        SupportedExtensions(0)
    }

    pub const fn all() -> Self {
        SupportedExtensions(u64::MAX)
    }

    pub const fn has(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub fn insert(&mut self, other: Self) -> &mut Self {
        self.0 |= other.0;
        self
    }

    pub fn remove(&mut self, other: Self) -> &mut Self {
        self.0 &= !other.0;
        self
    }
}

impl BitOr for SupportedExtensions {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        SupportedExtensions(self.0 | rhs.0)
    }
}

impl BitOrAssign for SupportedExtensions {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for SupportedExtensions {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self::Output {
        SupportedExtensions(self.0 & rhs.0)
    }
}

impl BitAndAssign for SupportedExtensions {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

impl Not for SupportedExtensions {
    type Output = Self;
    fn not(self) -> Self::Output {
        SupportedExtensions(!self.0)
    }
}

pub struct Registers {
    pub x0: u64,
    pub x1: u64,
    pub x2: u64,
    pub x3: u64,
    pub x4: u64,
    pub x5: u64,
    pub x6: u64,
    pub x7: u64,
    pub x8: u64,
    pub x9: u64,
    pub x10: u64,
    pub x11: u64,
    pub x12: u64,
    pub x13: u64,
    pub x14: u64,
    pub x15: u64,
    pub x16: u64,
    pub x17: u64,
    pub x18: u64,
    pub x19: u64,
    pub x20: u64,
    pub x21: u64,
    pub x22: u64,
    pub x23: u64,
    pub x24: u64,
    pub x25: u64,
    pub x26: u64,
    pub x27: u64,
    pub x28: u64,
    pub x29: u64,
    pub x30: u64,
    pub x31: u64,

    pub pc: usize,
}

pub struct PhysicalMemory {
    inner: Box<[u8]>,
}

impl PhysicalMemory {
    pub fn new(size: usize) -> Self {
        Self {
            inner: vec![0; size].into_boxed_slice()
        }
    }
}

pub struct WhiskerCpu {
    supported_extensions: SupportedExtensions
}

impl WhiskerCpu {
    pub fn new(supported_extensions: SupportedExtensions) -> Self {
        Self {
            supported_extensions,
        }
    }
}

impl Default for WhiskerCpu {
    fn default() -> Self {
        Self {
            supported_extensions: SupportedExtensions::all(),
        }
    }
}

fn main() {
    let mut cpu = WhiskerCpu::default();
}
