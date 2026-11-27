//! homeitem 前端入口（Leptos CSR）。

pub mod api;
pub mod app;
pub mod pages;

use leptos::mount::mount_to_body;
use leptos::*;
use wasm_bindgen::prelude::*;

/// Trunk/WASM 入口：`#[wasm_bindgen(start)]` 让 wasm-bindgen 在 init 后
/// 自动调用本函数挂载根组件；缺少它会导致页面空白。
#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(|| {
        view! {
            <crate::app::App />
        }
    });
}
