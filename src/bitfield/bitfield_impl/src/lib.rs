use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{DeriveInput, Error, Field, ItemEnum, ItemStruct};

#[proc_macro]
pub fn generate_basic_types(_: TokenStream) -> TokenStream {
	let tys = (1..=128).map(generate_basic_type);
	quote! {#(#tys)*}.into()
}

fn generate_basic_type(num_bits: usize) -> TokenStream2 {
	let io_ty = match num_bits {
		..=8 => quote!(::core::primitive::u8),
		..=16 => quote!(::core::primitive::u16),
		..=32 => quote!(::core::primitive::u32),
		..=64 => quote!(::core::primitive::u64),
		..=128 => quote!(::core::primitive::u128),
		_ => unreachable!(),
	};

	let ident = format_ident!("U{}", num_bits);
	// we can't produce T::MAX reliably especially for u128, so use them when possible
	let max_val = if num_bits.is_power_of_two() {
		quote! {{#io_ty::MAX}}
	} else {
		quote! {{((1 as #io_ty) << #num_bits) - 1}}
	};
	quote! {
		pub enum #ident {}

		impl crate::BitField for #ident {
			const SIZE: usize = #num_bits;
			type Storage = #io_ty;
			type IO = #io_ty;
			#[inline]
			fn from_bits(bits: <Self as crate::BitField>::Storage) -> <Self as crate::BitField>::IO {
				debug_assert!(bits <= #max_val, "{:b}", bits);
				bits
			}
			#[inline]
			fn to_bits(bits: <Self as crate::BitField>::IO) -> <Self as crate::BitField>::Storage {
				bits
			}
		}
	}
}

struct BitfieldStructInfo {
	input: ItemStruct,
}

impl BitfieldStructInfo {
	fn expand(self) -> TokenStream2 {
		let span = self.input.span();
		let vis = &self.input.vis;
		let ident = &self.input.ident;
		let size_expr = total_bit_size_expr(&self.input);
		let attrs = &self.input.attrs;

		let str = quote_spanned!(span =>
			#( #attrs )*
			#vis struct #ident {
				#[allow(unused, clippy::identity_op)]
				bytes: [::core::primitive::u8; #size_expr / 8_usize],
			}
		);

		let ctor = expand_ctor(&self.input);
		let getters = expand_accessors(&self.input);

		quote_spanned!(span => #str #ctor #getters)
	}
}

#[proc_macro_derive(BitFieldRepr)]
pub fn bitfield_repr(input: TokenStream) -> TokenStream {
	match bitfield_repr_inner(TokenStream2::from(input)) {
		Ok(ts) => ts.into(),
		Err(e) => e.into_compile_error().into(),
	}
}

fn bitfield_repr_inner(input: TokenStream2) -> syn::Result<TokenStream2> {
	let target = syn::parse2::<DeriveInput>(input)?;
	match target.data {
		syn::Data::Struct(_data_struct) => todo!("BitFieldRepr for structs?"),
		syn::Data::Enum(data_enum) => bitfield_repr_enum(ItemEnum {
			attrs: target.attrs,
			vis: target.vis,
			enum_token: data_enum.enum_token,
			ident: target.ident,
			generics: target.generics,
			brace_token: data_enum.brace_token,
			variants: data_enum.variants,
		}),
		syn::Data::Union(..) => Err(Error::new(target.span(), "unions are not supported as bitfield types")),
	}
}

fn bitfield_repr_enum(e: ItemEnum) -> syn::Result<TokenStream2> {
	let span = e.span();
	let ident = &e.ident;
	let (impl_generics, ty_generics, where_clauses) = e.generics.split_for_impl();

	let count = e.variants.iter().count();
	let bits = count.next_power_of_two().ilog2() as usize;

	for variant in e.variants.iter() {
		match &variant.fields {
			syn::Fields::Unit => (),
			_ => {
				return Err(Error::new(
					variant.span(),
					"bitfield repr enum variants must not have fields",
				));
			}
		}
	}

	let from_bits_arms = e.variants.iter().map(|variant| {
		let span = variant.span();
		let ident = &variant.ident;
		quote_spanned! {span =>
			__bitfield_bits
				if __bitfield_bits == Self::#ident as <Self as ::bitfield::BitField>::Storage => {
					Self::#ident
			}
		}
	});

	let storage_type = match bits {
		..=8 => quote!(::core::primitive::u8),
		..=16 => quote!(::core::primitive::u16),
		..=32 => quote!(::core::primitive::u32),
		..=64 => quote!(::core::primitive::u64),
		..=128 => quote!(::core::primitive::u128),
		_ => unreachable!(),
	};

	Ok(quote_spanned! {span =>
		#[automatically_derived]
		impl #impl_generics ::bitfield::BitField for #ident #ty_generics #where_clauses {
			const SIZE: usize = #bits;
			type Storage = #storage_type;
			type IO = Self;

			fn from_bits(bits: <Self as ::bitfield::BitField>::Storage) -> <Self as ::bitfield::BitField>::IO {
				match bits {
					#(#from_bits_arms)*
					__bitfield_bits => panic!("invalid bitfield value for type {}: {}", stringify!(#ident), __bitfield_bits),
				}
			}
			fn to_bits(val: <Self as ::bitfield::BitField>::IO) -> <Self as ::bitfield::BitField>::Storage {
				val as <Self as ::bitfield::BitField>::Storage
			}
		}
	})
}

#[proc_macro_attribute]
pub fn bitfields(_args: TokenStream, input: TokenStream) -> TokenStream {
	let item = match syn::parse::<ItemStruct>(input) {
		Ok(item) => item,
		Err(e) => return e.into_compile_error().into(),
	};
	BitfieldStructInfo { input: item }.expand().into()
}

/// returns an expression calculating the total size of the struct, in bits
/// the expression will evaluate to a value that is a multiple of 8
fn total_bit_size_expr(s: &ItemStruct) -> TokenStream2 {
	let span = s.span();
	let bits_expr = s.fields.iter().map(field_size_expr).fold(
		quote_spanned!(span => 0_usize),
		// using a TS in quote inlines the token streams, so this just generates a bunch of
		// expressions from field_size_expr inline, separated by +
		|acc, it| quote_spanned!(span => #acc + #it),
	);

	// align number of bits up to 8
	quote_spanned!(span => (((#bits_expr) + (8-1)) & !(8-1)))
}

fn field_size_expr(field: &Field) -> TokenStream2 {
	let span = field.span();
	let ty = &field.ty;

	quote_spanned! {span => <#ty as ::bitfield::BitField>::SIZE}
}

fn expand_ctor(s: &ItemStruct) -> TokenStream2 {
	let span = s.span();
	let ident = &s.ident;
	let (impl_generics, ty_generics, where_clauses) = s.generics.split_for_impl();
	let size_expr = total_bit_size_expr(s);

	quote_spanned! {span =>
		impl #impl_generics #ident #ty_generics #where_clauses {
			#[allow(dead_code, clippy::identity_op)]
			#[must_use]
			pub const fn new() -> Self {
				Self {
					bytes: [0_u8; #size_expr / 8_usize],
				}
			}
		}
	}
}

fn expand_accessors(s: &ItemStruct) -> TokenStream2 {
	let span = s.span();
	let ident = &s.ident;
	let size_expr = total_bit_size_expr(s);
	let mut bit_offset_expr = quote_spanned!(span => 0_usize);

	let accessors = s.fields.iter().map(|f| {
		let ty = &f.ty;
		let getter = getter_for_field(f, &bit_offset_expr);
		let setter = setter_for_field(f, &bit_offset_expr);

		bit_offset_expr.extend(quote_spanned!(span => + <#ty as ::bitfield::BitField>::SIZE));

		quote_spanned!(span => #getter #setter)
	});

	quote_spanned!(span => impl #ident {
		#[allow(dead_code, clippy::identity_op)]
		pub fn inner(&self) -> [::core::primitive::u8; #size_expr / 8_usize] {
			self.bytes
		}

		#[allow(dead_code, clippy::identity_op)]
		pub fn set_inner(&mut self, val: [::core::primitive::u8; #size_expr / 8_usize]) {
			self.bytes = val;
		}

		#( #accessors )*
	})
}

fn getter_for_field(f: &Field, offset: &TokenStream2) -> TokenStream2 {
	let span = f.span();
	let ty = &f.ty;
	let vis = &f.vis;
	let get_ident = format_ident!("get_{}", f.ident.clone().unwrap());

	quote_spanned!(span =>
		#[allow(non_snake_case, dead_code, clippy::identity_op)]
		#vis fn #get_ident(&self) -> <#ty as ::bitfield::BitField>::IO {
			::bitfield::private_impl::read_val::<#ty>(&self.bytes, #offset)
		}
	)
}

fn setter_for_field(f: &Field, offset: &TokenStream2) -> TokenStream2 {
	let span = f.span();
	let ty = &f.ty;
	let vis = &f.vis;
	let set_ident = format_ident!("set_{}", f.ident.clone().unwrap());

	quote_spanned!(span =>
		#[allow(non_snake_case, dead_code, clippy::identity_op)]
		#vis fn #set_ident(&mut self, val: <#ty as ::bitfield::BitField>::IO) {
			::bitfield::private_impl::write_val::<#ty>(&mut self.bytes, val, #offset)
		}
	)
}
