use hearth_reflect_derive::{impl_reflect_opaque, impl_type_path};

impl_type_path!(::hearth_platform::hash::NoOpHash);
impl_type_path!(::hearth_platform::hash::FixedHasher);
impl_type_path!(::hearth_platform::hash::PassHash);

impl_reflect_opaque!(::hearth_platform::hash::Hashed<T: Clone + Send + Sync, H: Send + Sync>());
