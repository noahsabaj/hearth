#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/noahsabaj/hearth/main/assets/branding/icon.png",
    html_favicon_url = "https://raw.githubusercontent.com/noahsabaj/hearth/main/assets/branding/icon.png"
)]

//! A collection of helper types and functions for working on macros within the Hearth ecosystem.

extern crate alloc;
extern crate proc_macro;

mod attrs;
pub mod fq_std;
mod hearth_manifest;
mod label;
mod member;
mod parser;
mod path;
mod path_type;
mod result_sifter;
mod shape;
mod symbol;

pub use attrs::*;
pub use hearth_manifest::*;
pub use label::*;
pub use member::*;
pub use parser::*;
pub use path::*;
pub use path_type::*;
pub use result_sifter::*;
pub use shape::*;
pub use symbol::*;
