use super::upcast::*;

struct S;

impl S {
    fn s(&self) -> u32 {
        3
    }
}

trait A {
    fn a(&self) -> u32;
}
impl A for S {
    fn a(&self) -> u32 {
        1
    }
}

trait B {
    fn b(&self) -> u32;
}
impl B for S {
    fn b(&self) -> u32 {
        2
    }
}

impl_castable!(S: A, B);

#[test]
fn test_castable() {
    let s = S {};

    let c: &Castable = &s;

    assert_eq!(Some(1), try_cast::<A>(c).map(|a: &A| a.a()));
    assert_eq!(Some(2), try_cast::<B>(c).map(|b: &B| b.b()));
    assert_eq!(Some(3), try_cast::<S>(c).map(|s: &S| s.s()));
    assert_eq!(None, try_cast::<Option<u32>>(c));
    assert_eq!(None, try_cast::<()>(c));
}

#[test]
fn test_castable_box() {

    let c: Box<Castable> = Box::new(S);

    assert_eq!(Some(1), try_cast::<A>(&*c).map(|a: &A| a.a()));
    assert_eq!(Some(2), try_cast::<B>(&*c).map(|b: &B| b.b()));
    assert_eq!(Some(3), try_cast::<S>(&*c).map(|s: &S| s.s()));
    assert_eq!(None, try_cast::<Option<u32>>(&*c));
    assert_eq!(None, try_cast::<()>(&*c));
}
