//! Qualification reuses the production decoder and its regression tests.
#[path = "../../../nebula_app/src/gpui_shell/wallpaper/gif_decoder.rs"]
mod production;
pub use production::{Cursor, Decoded};
