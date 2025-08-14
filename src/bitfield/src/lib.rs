pub use bitfield_impl::{BitFieldRepr, bitfields};

pub mod prelude {
	pub use crate::types::*;
	pub use bitfield_impl::BitFieldRepr;
	pub use bitfield_impl::bitfields;
}

pub trait BitField {
	const SIZE: usize;
	type Storage: Default;
	type IO;

	fn from_bits(bits: Self::Storage) -> Self::IO;
	fn to_bits(val: Self::IO) -> Self::Storage;
}

pub mod types {
	bitfield_impl::generate_basic_types!();
}

impl BitField for bool {
	const SIZE: usize = 1;
	type Storage = u8;
	type IO = bool;
	fn from_bits(bits: Self::Storage) -> Self::IO {
		debug_assert!(bits < 2);
		bits != 0
	}
	fn to_bits(val: Self::IO) -> Self::Storage {
		u8::from(val)
	}
}

/*
pub enum U1 {}
impl BitField for U1 {
	const SIZE: usize = 1;
	type Storage = u8;
	type IO = u8;
	fn from_bits(bits: Self::Storage) -> Self::IO {
		todo!()
	}
	fn to_bits(val: Self::IO) -> Self::Storage {
		todo!()
	}
}
*/

#[doc(hidden)]
pub mod private_impl {
	use super::*;

	// a lot of these things will be called from other crates, so make sure
	// that they are marked for cross-crate inlining if possible

	#[inline]
	#[doc(hidden)]
	pub fn read_val<T>(bytes: &[u8], offset: usize) -> T::IO
	where
		T: BitField,
		BitCollector<T::Storage>: BitCollectorImpl,
	{
		let end = offset + T::SIZE - 1;
		let (start_idx, start_bit_idx) = offset_to_idx(offset);
		let (end_idx, end_bit_idx) = offset_to_idx(end);

		let mut collector = BitCollector(<T::Storage>::default());

		// handle potential high byte
		if end_idx > start_idx {
			let mask = (1_u8 << (end_bit_idx + 1)) - 1;
			collector.push_bits(bytes[end_idx] & mask, end_bit_idx + 1);
		}
		// handle middle bytes
		if end_idx - start_idx >= 2 {
			for idx in ((start_idx + 1)..=(end_idx - 1)).rev() {
				collector.push_bits(bytes[idx], 8);
			}
		}
		// handle least significant byte
		if start_idx == end_idx {
			// if the indices are the same, make sure to only get T::SIZE bits
			let mask = ((1_u8 << T::SIZE) - 1) << start_bit_idx;
			collector.push_bits((bytes[start_idx] & mask) >> start_bit_idx, T::SIZE);
		} else {
			// if the indices are not the same, get from start_bit_idx to the end of the byte
			let mask = 0xFF_u8 << start_bit_idx;
			collector.push_bits((bytes[start_idx] & mask) >> start_bit_idx, 8 - start_bit_idx);
		}

		T::from_bits(collector.0)
	}

	#[inline]
	#[doc(hidden)]
	pub fn write_val<T>(bytes: &mut [u8], val: T::IO, offset: usize)
	where
		T: BitField,
		BitReader<T::Storage>: BitReaderImpl,
	{
		let end = offset + T::SIZE - 1;
		let (start_idx, start_bit_idx) = offset_to_idx(offset);
		let (end_idx, end_bit_idx) = offset_to_idx(end);

		let mut bit_reader = BitReader(T::to_bits(val));

		// handle least significant byte
		if start_idx == end_idx {
			// if the indices are the same, make sure to only set T::SIZE bits
			let mask = !(((1_u8 << T::SIZE) - 1) << start_bit_idx);
			bytes[start_idx] &= mask;
			bytes[start_idx] |= bit_reader.read_bits(T::SIZE) << start_bit_idx;
		} else {
			// if the indices are not the same, set from start_bit_idx to the end of the byte
			let mask = !(0xFF_u8 << start_bit_idx);
			bytes[start_idx] &= mask;
			let val = bit_reader.read_bits(8 - start_bit_idx);
			bytes[start_idx] |= val << start_bit_idx;
		}

		// handle middle bytes
		if end_idx - start_idx >= 2 {
			for idx in ((start_idx + 1)..=(end_idx - 1)).rev() {
				let val = bit_reader.read_bits(8);
				bytes[idx] = val;
			}
		}

		// handle potential high byte
		if end_idx > start_idx {
			let mask = !((1_u8 << (end_bit_idx + 1)) - 1);
			bytes[end_idx] &= mask;
			let val = bit_reader.read_bits(end_bit_idx + 1);
			bytes[end_idx] |= val;
		}
	}

	#[inline(always)]
	fn offset_to_idx(offset: usize) -> (usize, usize) {
		(offset / 8, offset % 8)
	}

	/// helper to generically collect bits for [`read_val`]
	#[doc(hidden)]
	pub trait BitCollectorImpl {
		fn push_bits(&mut self, bits: u8, num_bits: usize);
	}

	#[doc(hidden)]
	pub struct BitCollector<T>(T);

	macro_rules! impl_bit_collector {
		($($ty:ty),+) => {
		    $(impl BitCollectorImpl for BitCollector<$ty> {
				#[inline]
				fn push_bits(&mut self, bits: u8, num_bits: usize) {
					self.0 = (self.0 << num_bits) | <$ty>::from(bits);
				}
			})+
		};
	}

	impl_bit_collector!(u8, u16, u32, u64, u128);

	#[doc(hidden)]
	pub trait BitReaderImpl {
		fn read_bits(&mut self, num_bits: usize) -> u8;
	}

	#[doc(hidden)]
	pub struct BitReader<T>(T);

	macro_rules! impl_bit_reader {
        ($($ty:ty),+) => {
            $(impl BitReaderImpl for BitReader<$ty> {
                #[inline]
                fn read_bits(&mut self, num_bits: usize) -> u8 {
                    let mask = ((1_u16 << num_bits) - 1) as u8;
                    let ret = self.0 as u8 & mask;
                    self.0 >>= num_bits;
                    ret
                }
            })+
        };
	}

	impl_bit_reader!(u8, u16, u32, u64, u128);
}
