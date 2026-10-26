//! A small web engine for the ticly browser on arcOS.
//!
//! It reads HTML into a tree ([`html`], [`dom`]), applies CSS ([`css`],
//! [`style`]), lays the page out in boxes and lines ([`layout`]) and paints
//! it on an [`arcgfx::Canvas`]. Pictures come from its own PNG, JPEG and GIF
//! decoders ([`image`]). It runs no scripts: pages are shown as their HTML
//! and CSS describe them, which is what most articles, documentation,
//! search results and forms need.
//!
//! The engine does no input or output: the host fetches the stylesheets and
//! pictures a [`Document`] asks for and hands them back, and draws with the
//! faces of a [`fonts::WebFonts`] atlas.

#![no_std]

extern crate alloc;
#[cfg(test)]
extern crate std;

pub mod css;
pub mod document;
pub mod dom;
pub mod fonts;
pub mod html;
pub mod image;
pub mod layout;
pub mod style;
pub mod url;

pub use document::{Document, Hit};
pub use fonts::WebFonts;

/// Rounding for `f32` without the standard library.
pub(crate) trait Round {
    fn flr(self) -> f32;
    fn cel(self) -> f32;
    fn rnd(self) -> f32;
}

impl Round for f32 {
    fn flr(self) -> f32 {
        if !(-1e9..=1e9).contains(&self) {
            return self;
        }
        let i = self as i64 as f32;
        if i > self {
            i - 1.0
        } else {
            i
        }
    }

    fn cel(self) -> f32 {
        -(-self).flr()
    }

    fn rnd(self) -> f32 {
        (self + 0.5).flr()
    }
}
