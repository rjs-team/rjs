// this is an attempt to make a way to allow an Any-style trait object be casted to a number of
// trait types

use std::any::TypeId;

pub fn try_cast<T: ?Sized + 'static>(c: &Castable) -> Option<&T> {
    let mut v: Option<&T> = None;

    unsafe { c.unsafe_try_cast(TypeId::of::<T>(), &mut v as *mut Option<&T> as *mut _) }

    v
}

pub trait Castable {
    unsafe fn unsafe_try_cast(&self, to: TypeId, out: *mut Option<()>);
}

#[macro_export]
macro_rules! impl_castable {
    ($t:ty : $($tr:ty),*) => {
        impl Castable for $t {
            unsafe fn unsafe_try_cast(&self, to: ::std::any::TypeId, out: *mut Option<()>) {
                if to == ::std::any::TypeId::of::<$t>() {
                    *(out as *mut Option<&$t>) = Some(self as &$t);
                    return;
                }
                $(
                if to == ::std::any::TypeId::of::<$tr>() {
                    *(out as *mut Option<&$tr>) = Some(self as &$tr);
                    return;
                }
                )*
            }
        }
    };
}
