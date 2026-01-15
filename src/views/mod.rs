//! HTML view modules for traceview using maud templating.

mod layout;
mod sessions;
mod trace;

pub use layout::base_layout;
pub use sessions::sessions_list;
pub use trace::{session_detail, span_html};
