//! What the human control plane prints for a person to read: tables with a
//! header, and bold that only reaches a terminal.

pub mod style;
pub mod table;

pub use table::Table;
