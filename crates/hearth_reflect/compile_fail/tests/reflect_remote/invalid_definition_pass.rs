//@check-pass

mod structs {
    use hearth_reflect::reflect_remote;

    mod external_crate {
        pub struct TheirStruct {
            pub value: u32,
        }
    }

    #[reflect_remote(external_crate::TheirStruct)]
    struct MyStruct {
        pub value: u32,
    }
}

mod tuple_structs {
    use hearth_reflect::reflect_remote;

    mod external_crate {
        pub struct TheirStruct(pub u32);
    }

    #[reflect_remote(external_crate::TheirStruct)]
    struct MyStruct(pub u32);
}

mod enums {
    use hearth_reflect::reflect_remote;

    mod external_crate {
        pub enum TheirStruct {
            Unit,
            Tuple(u32),
            Struct { value: usize },
        }
    }

    #[reflect_remote(external_crate::TheirStruct)]
    enum MyStruct {
        Unit,
        Tuple(u32),
        Struct { value: usize },
    }
}

fn main() {}
