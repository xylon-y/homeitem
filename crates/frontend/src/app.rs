//! 根组件：路由表 + 全局布局 + 登录态管理。

use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

use crate::pages::{dashboard, items, login, maintenance, reminders, settings, setup, taxonomy};

/// 全局登录态：任何组件都可读写，401 时置 false 触发重新渲染。
#[derive(Clone, Copy)]
pub struct AuthSignal(pub RwSignal<bool>);

/// 未登录时的路由（登录/初始化/注册）。
#[component]
fn GuestRoutes() -> impl IntoView {
    view! {
        <Routes fallback=|| view! { <login::LoginPage /> }>
            <Route path=path!("/setup") view=setup::SetupPage />
            <Route path=path!("/register") view=login::RegisterPage />
            <Route path=path!("/*any") view=login::LoginPage />
        </Routes>
    }
}

/// 登录后的主路由。
#[component]
fn MainRoutes() -> impl IntoView {
    view! {
        <Routes fallback=|| view! { <div class="page"><p class="error">页面不存在</p></div> }>
            <Route path=path!("/") view=dashboard::DashboardPage />
            <Route path=path!("/items") view=items::ItemsPage />
            <Route path=path!("/items/new") view=items::ItemFormPage />
            <Route path=path!("/items/:id") view=items::ItemDetailPage />
            <Route path=path!("/items/:id/edit") view=items::ItemFormPage />
            <Route path=path!("/maintenance/:id") view=maintenance::MaintenancePage />
            <Route path=path!("/locations") view=items::LocationsPage />
            <Route path=path!("/tags") view=taxonomy::TagsPage />
            <Route path=path!("/types") view=taxonomy::TypesPage />
            <Route path=path!("/reminders") view=reminders::RemindersPage />
            <Route path=path!("/notifiers") view=reminders::NotifiersPage />
            <Route path=path!("/settings") view=settings::SettingsPage />
        </Routes>
    }
}

/// 应用根：响应式登录态 + 路由分发。
#[component]
pub fn App() -> impl IntoView {
    let logged_in = RwSignal::new(crate::api::token::get().is_some());
    provide_context(AuthSignal(logged_in));

    // 定时轮询：捕获 api 层标记的 401，统一登出并跳转登录页。
    let _tick = set_interval_with_handle(
        move || {
            if crate::api::take_unauthorized() {
                logged_in.set(false);
                if let Some(w) = web_sys::window() {
                    let _ = w.location().set_href("/login");
                }
            }
        },
        std::time::Duration::from_millis(500),
    );

    let logout_action = move |_| {
        crate::api::logout();
        logged_in.set(false);
        if let Some(w) = web_sys::window() {
            let _ = w.location().set_href("/login");
        }
    };

    view! {
        <Router>
            <div class="app-shell">
                <Show when={move || logged_in.get()} fallback=|| view! { <GuestRoutes /> }>
                    <nav class="topbar">
                        <a class="brand" href="/">"homeitem 物品管理"</a>
                        <div class="nav-links">
                            <a href="/">"仪表盘"</a>
                            <a href="/items">"物品"</a>
                            <a href="/locations">"位置"</a>
                            <a href="/tags">"标签"</a>
                            <a href="/reminders">"提醒"</a>
                            <a href="/settings">"设置"</a>
                            <button class="btn-link" on:click=logout_action>"退出"</button>
                        </div>
                    </nav>
                    <main class="content">
                        <MainRoutes />
                    </main>
                </Show>
            </div>
        </Router>
    }
}
