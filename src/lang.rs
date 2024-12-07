//! Language support.

use core::iter::zip;
use core::mem::MaybeUninit;
use core::ptr::NonNull;

use alloc::boxed::Box;
use enumset::EnumSet;

use crate::compile::{self, Ccx};
use crate::emit::{Ecx, Emit, InsValue};
use crate::foreach_lang;
use crate::ir::{Func, InsId};
use crate::lower::CLcx;
use crate::obj::{ObjRef, CALLX};
use crate::parser::Pcx;

pub trait Language: Sized {
    fn parse(pcx: &mut Pcx, n: usize) -> compile::Result<ObjRef<CALLX>>;
    fn lower(lcx: &mut CLcx, obj: ObjRef<CALLX>, func: &Func, inputs: &[InsId]) -> InsId;
    fn begin_emit(ccx: &mut Ccx) -> compile::Result<Self>;
    fn emit(ecx: &mut Ecx, id: InsId, lop: u8) -> compile::Result<InsValue>;
    #[allow(unused_variables)]
    fn finish_emit(self, ccx: &mut Ccx<Emit>) -> compile::Result { Ok(()) }
}

macro_rules! define_langs {
    ( $($(#[$($meta:tt)*])? $module:ident::$name:ident;)* ) => {
        #[derive(enumset::EnumSetType)]
        #[repr(u8)]
        pub enum Lang {
            $($name),*
        }

        impl Lang {

            pub fn from_name(name: &[u8]) -> Option<Self> {
                $(
                    $(#[$($meta)*])?
                    const $name: &[u8] = stringify!($name).as_bytes();
                )*
                match name {
                    $(
                        $(#[$($meta)*])?
                        $name => Some(Lang::$name),
                    )*
                        _ => None
                }
            }

            pub fn name(self) -> &'static str {
                match self {
                    $(
                        $(#[$($meta)*])?
                        Self::$name => stringify!($name),
                    )*
                }
            }

        }

        #[repr(C)]
        union AnyLang {
            $(
                $(#[$($meta)*])?
                $name: core::mem::ManuallyDrop<$crate::$module::$name>,
            )*
        }

        impl LangState {
            $(
                $(#[$($meta)*])?
                #[allow(dead_code)]
                pub fn $name(&mut self) -> &mut $crate::$module::$name {
                    unsafe { &mut self.get_mut(Lang::$name).$name }
                }
            )*
        }

        macro_rules! dispatch {
            ($discrim:expr, $lang:ty => $value:expr) => {
                match $discrim {
                    $(
                        $(#[$($meta)*])?
                        Lang::$name => { type Lang = $crate::$module::$name; $value },
                    )*
                }
            }
        }
    };
}

foreach_lang!(define_langs);

impl Lang {

    pub fn from_u8(raw: u8) -> Self {
        // FIXME replace with core::mem::variant_count when it stabilizes
        assert!(raw < <Self as enumset::__internal::EnumSetTypePrivate>::VARIANT_COUNT as _);
        unsafe { core::mem::transmute(raw) }
    }

    pub fn parse(self, pcx: &mut Pcx, n: usize) -> compile::Result<ObjRef<CALLX>> {
        dispatch!(self, Lang => Lang::parse(pcx, n))
    }

    pub fn lower(
        self,
        lcx: &mut CLcx,
        obj: ObjRef<CALLX>,
        func: &Func,
        inputs: &[InsId]
    ) -> InsId {
        dispatch!(self, Lang => Lang::lower(lcx, obj, func, inputs))
    }

    pub fn emit(self, ecx: &mut Ecx, id: InsId, lop: u8) -> compile::Result<InsValue> {
        dispatch!(self, Lang => Lang::emit(ecx, id, lop))
    }

}

// note: this intentionally does *not* implement Drop. you "drop" it by calling `finish`.
pub struct LangState {
    present: EnumSet<Lang>,
    data: NonNull<AnyLang>
}

impl Default for LangState {
    fn default() -> Self {
        Self {
            present: EnumSet::empty(),
            data: NonNull::dangling()
        }
    }
}

impl LangState {

    pub fn new(ccx: &mut Ccx, langs: EnumSet<Lang>) -> compile::Result<Self> {
        let num = langs.len();
        if num == 0 {
            return Ok(Default::default());
        }
        // XXX: replace this with try_collect() when it stabilizes.
        let mut mem: Box<[MaybeUninit<AnyLang>]> = Box::new_uninit_slice(num);
        for (ptr,lang) in zip(mem.iter_mut(), langs.into_iter()) {
            dispatch!(lang, Lang => {
                let l = Lang::begin_emit(ccx)?;
                unsafe { ptr.as_mut_ptr().cast::<Lang>().write(l) }
            });
        }
        Ok(Self {
            present: langs,
            data: unsafe { NonNull::new_unchecked(Box::leak(mem) as *mut _ as *mut _) }
        })
    }

    pub fn finish(self, ccx: &mut Ccx<Emit>) -> compile::Result {
        let mut mem = unsafe {
            Box::from_raw(
                core::ptr::slice_from_raw_parts_mut(self.data.as_ptr(), self.present.len())
            )
        };
        let mut result = Ok(());
        for (ptr,lang) in zip(mem.iter_mut(), self.present) {
            dispatch!(lang, Lang => {
                let l = unsafe { (ptr as *mut AnyLang).cast::<Lang>().read() };
                result = result.and(Lang::finish_emit(l, ccx));
            });
        }
        result
    }

    fn get_mut(&mut self, lang: Lang) -> &mut AnyLang {
        assert!(self.present.contains(lang));
        let idx = (self.present.as_u64_truncated() & ((1 << lang as u8) - 1)).count_ones();
        unsafe { &mut *self.data.as_ptr().add(idx as usize) }
    }

}

// use core::alloc::Layout;
// use core::convert::Infallible;
// use core::marker::PhantomData;
// use core::mem::{forget, ManuallyDrop};
// use core::ptr::{copy_nonoverlapping, NonNull};
// 
// use alloc::vec::Vec;
// use enumset::EnumSet;
// 
// use crate::parser::Pcx;
// use crate::{compile, foreach_lang};
// 
// pub trait Language {
//     const NAME: &'static [u8];
//     type ObjData;
//     type EmitData;
//     type ImageData;
//     fn graph_data() -> Self::ObjData;
//     fn parse_call(data: &mut Self::ObjData, pcx: &mut Pcx) -> compile::Result;
//     // need type_call?
//     // fn lower_call(ccx: &mut Ccx<Lower>, call: ExprId) -> compile::Result;
//     // need sink_call?
// }
// 
// macro_rules! define_lang_enum {
//     ($($(#[$($meta:tt)*])? $module:ident::$name:ident;)*) => {
//         #[derive(enumset::EnumSetType, Debug)]
//         #[repr(u8)]
//         pub enum Lang {
//             $($name,)*
//         }
// 
//         impl Lang {
//             pub fn for_name(name: &[u8]) -> Option<Self> {
//                 match name {
//                     $(
//                         $(#[$($meta)*])?
//                         <crate::$module::$name as Language>::NAME => Some(Lang::$name),
//                     )*
//                     _ => None
//                 }
//             }
//         }
//     };
// }
// 
// foreach_lang!(define_lang_enum);
// 
// // more or less equivalent to the rust enum corresponding to the union `T`,
// // with a variant for each language.
// pub struct Tagged<T> {
//     lang: Lang,
//     value: T
// }
// 
// impl<T> Tagged<T> {
// 
//     pub fn lang(&self) -> Lang {
//         self.lang
//     }
// 
// }
// 
// macro_rules! define_union {
//     (
//         pub union $name:ident(Language::$field:ident)
//         $($(#[$($attr:tt)*])? $module:ident::$lang:ident;)*
//     ) => {
//         pub union $name {
//             $(
//                 $(#[$($attr)*])?
//                 $lang: core::mem::ManuallyDrop<<crate::$module::$lang as Language>::$name>,
//             )*
//         }
//         impl ULang for $name {
//             fn drop_in_place(e: Tagged<&mut Self>) {
//                 match e.lang {
//                     $(
//                         $(#[$($attr)*])?
//                         Lang::$lang => unsafe { core::ptr::drop_in_place(&mut e.value.$lang) },
//                     )*
//                 }
//             }
//         }
//     };
// }
// 
// foreach_lang!(define_union, pub union ObjData(Language::ObjData));
// foreach_lang!(define_union, pub union EmitData(Language::EmitData));
// foreach_lang!(define_union, pub union ImageData(Language::ImageData));
// 
// macro_rules! define_dispatch {
//     ($($(#[$($attr:tt)*])? $module:ident::$lang:ident;)*) => {
//         macro_rules! dispatch {
//             ($discrim:expr => $value:ident) => {
//                 match $discrim {
//                     $(
//                         $(#[$($attr)*])?
//                         Lang::$lang => $value!($module::$lang),
//                     )*
//                 }
//             }
//         }
//     };
// }
// 
// foreach_lang!(define_dispatch);
// 
// impl Lang {
// 
//     pub fn name(self) -> &'static [u8] {
//         macro_rules! value {
//             ($module:ident::$lang:ident) => { <crate::$module::$lang as Language>::NAME };
//         }
//         dispatch!(self => value)
//     }
// 
//     pub fn graph_data(self) -> Tagged<ObjData> {
//         macro_rules! value {
//             ($module:ident::$lang:ident) => {
//                 ObjData {
//                     $lang: core::mem::ManuallyDrop::new(
//                         <crate::$module::$lang as Language>::graph_data())
//                 }
//             };
//         }
//         Tagged { lang: self, value: dispatch!(self => value) }
//     }
// 
// }
// 
// impl Tagged<&mut ObjData> {
// 
//     pub fn parse_call(self, pcx: &mut Pcx) -> compile::Result {
//         macro_rules! value {
//             ($module:ident::$lang:ident) => {
//                 <crate::$module::$lang as Language>::parse_call(
//                     unsafe { &mut self.value.$lang }, pcx
//                 )
//             };
//         }
//         dispatch!(self.lang => value)
//     }
// 
// }
// 
// pub trait ULang {
//     fn drop_in_place(v: Tagged<&mut Self>);
// }
// 
// pub struct LangMap<U: ULang> {
//     mask: EnumSet<Lang>,
//     ptr: NonNull<u8>, // untyped so that new etc. don't get copied
//     _type: PhantomData<U>
// }
// 
// impl<U: ULang> LangMap<U> {
// 
//     #[cold]
//     unsafe fn _new(&mut self, lang: Lang, size: usize) -> *mut u8 {
//         let mask = self.mask.as_u64_truncated();
//         let tail = (mask >> (lang as usize)).count_ones() as usize;
//         let head = mask.count_ones() as usize - tail;
//         let total = head + tail + 1;
//         let ptr = NonNull::new(if mask == 0 {
//             alloc::alloc::alloc(Layout::from_size_align_unchecked(total*size, 8))
//         } else {
//             alloc::alloc::realloc(
//                 self.ptr.as_ptr(),
//                 Layout::from_size_align_unchecked((head+tail)*size, 8),
//                 total*size
//             )
//         }).unwrap();
//         if tail > 0 {
//             core::ptr::copy(ptr.as_ptr().add(head*size), ptr.as_ptr().add((head+1)*size), tail*size);
//         }
//         self.mask.insert(lang);
//         self.ptr = ptr;
//         ptr.as_ptr().add(head*size)
//     }
// 
//     unsafe fn new(&mut self, lang: Lang) -> NonNull<U> {
//         NonNull::new_unchecked(self._new(lang, size_of::<U>()).cast())
//     }
// 
//     fn data_ptr(&self) -> *mut U {
//         self.ptr.as_ptr() as _
//     }
// 
//     fn get_ptr(&self, lang: Lang) -> Option<NonNull<U>> {
//         let mask = self.mask.as_u64_truncated();
//         if (mask & (1 << lang as u64)) != 0 {
//             let idx = (mask & ((1 << lang as u64) - 1)).count_ones() as usize;
//             Some(unsafe { NonNull::new_unchecked(self.data_ptr().add(idx)) })
//         } else {
//             None
//         }
//     }
// 
//     pub fn insert(&mut self, entry: Tagged<U>) {
//         let ptr = match self.get_ptr(entry.lang) {
//             Some(ptr) => ptr,
//             None => unsafe { self.new(entry.lang) }
//         };
//         *unsafe { &mut *ptr.as_ptr() } = entry.value;
//     }
// 
//     pub fn contains(&self, lang: Lang) -> bool {
//         self.mask.contains(lang.into())
//     }
// 
//     pub fn get_mut(&mut self, lang: Lang) -> Option<Tagged<&mut U>> {
//         match self.get_ptr(lang) {
//             Some(ptr) => Some(Tagged { lang, value: unsafe { &mut *ptr.as_ptr() } }),
//             None      => None
//         }
//     }
// 
//     pub fn iter_mut(&mut self) -> impl Iterator<Item=Tagged<&mut U>> {
//         self.mask.iter()
//             .enumerate()
//             .map(|(idx,lang)| Tagged { lang, value: unsafe { &mut *self.data_ptr().add(idx) }})
//     }
// 
//     // not trait because i can't be bothered trying to type out the return type
//     pub fn into_iter(self) -> impl Iterator<Item=Tagged<U>> {
//         let this = ManuallyDrop::new(self);
//         this.mask
//             .iter()
//             .enumerate()
//             .map(move |(idx,lang)| Tagged {
//                 lang,
//                 value: unsafe { core::ptr::read(this.data_ptr().add(idx))
//             }})
//     }
// 
//     pub fn try_from_iter<I,E>(iter: I) -> Result<Self, E>
//         where I: Iterator<Item=Result<Tagged<U>, E>>
//     {
//         // needless allocation here that could easily be eliminated but not worth any effort
//         // since this function is called like twice during the entire compiler pipeline.
//         let mut buf: Vec<Tagged<U>> = Default::default();
//         let mut mask: EnumSet<Lang> = Default::default();
//         buf.reserve_exact(iter.size_hint().0);
//         for e in iter {
//             let e = e?;
//             mask |= e.lang;
//             buf.push(e);
//         }
//         Ok(if mask.is_empty() {
//             Default::default()
//         } else {
//             let ptr = unsafe {
//                 alloc::alloc::alloc(Layout::from_size_align_unchecked(mask.len()*size_of::<U>(), 8))
//             };
//             let mask64 = mask.as_u64_truncated();
//             for e in &buf {
//                 unsafe {
//                     copy_nonoverlapping(
//                         &e.value,
//                         ptr.cast::<U>().add(
//                             (mask64 & ((1 << (e.lang as usize))-1)).count_ones() as usize
//                         ),
//                         1
//                     );
//                 }
//             }
//             forget(buf);
//             Self {
//                 mask,
//                 ptr: unsafe { NonNull::new_unchecked(ptr) },
//                 _type: PhantomData
//             }
//         })
//     }
// 
//     pub fn from_iter<I>(iter: I) -> Self
//         where I: Iterator<Item=Tagged<U>>
//     {
//         Self::try_from_iter::<_, Infallible>(iter.map(Ok)).unwrap()
//     }
// 
// }
// 
// impl<U: ULang> Default for LangMap<U> {
//     fn default() -> Self {
//         Self {
//             mask: EnumSet::empty(),
//             ptr: NonNull::dangling(),
//             _type: PhantomData
//         }
//     }
// }
// 
// impl<U: ULang> Drop for LangMap<U> {
//     fn drop(&mut self) {
//         for e in self.iter_mut() {
//             U::drop_in_place(e)
//         }
//     }
// }
