//! homeitem 前端入口（Leptos CSR）。

pub mod api;
pub mod app;
pub mod pages;

use leptos::mount::mount_to_body;
use leptos::*;

/// Trunk 入口：挂载根组件。
pub fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(|| {
        view! {
            <crate::app::App />
        }
    });
}
