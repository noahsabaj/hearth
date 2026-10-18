#![expect(
    unused_qualifications,
    reason = "the macro uses `MyEnum::Variant` which is generally unnecessary for `Result`"
)]

use hearth_reflect_derive::impl_reflect;

impl_reflect! {
    #[type_path = "core::result"]
    enum Result<T, E> {
        Ok(T),
        Err(E),
    }
}
