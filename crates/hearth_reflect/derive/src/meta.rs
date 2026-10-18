use hearth_macro_utils::HearthManifest;
use syn::Path;

/// Returns the correct path for `hearth_reflect`.
pub(crate) fn get_hearth_reflect_path() -> Path {
    HearthManifest::shared(|manifest| manifest.get_path("hearth_reflect"))
}
