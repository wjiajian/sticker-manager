//! macOS 菜单栏；业务菜单项与托盘共用 entry-* 分发。
use tauri::{
    AppHandle,
    menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem as Native, Submenu},
};

pub fn build(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let info = app.package_info();
    Menu::with_items(
        app,
        &[
            &Submenu::with_items(
                app,
                &info.name,
                true,
                &[
                    &Native::about(
                        app,
                        Some(&format!("关于{}", info.name)),
                        Some(AboutMetadata {
                            name: Some(info.name.clone()),
                            version: Some(info.version.to_string()),
                            // 不再从 CFBundleVersion 补上重复的括号版本号。
                            short_version: Some(String::new()),
                            ..Default::default()
                        }),
                    )?,
                    &Native::separator(app)?,
                    &MenuItem::with_id(app, "entry-preferences", "偏好设置…", true, Some("Cmd+,"))?,
                    &Native::separator(app)?,
                    &Native::hide(app, Some("隐藏"))?,
                    &Native::hide_others(app, Some("隐藏其他"))?,
                    &Native::show_all(app, Some("全部显示"))?,
                    &Native::separator(app)?,
                    &MenuItem::with_id(app, "entry-quit", "退出", true, Some("Cmd+Q"))?,
                ],
            )?,
            &Submenu::with_items(
                app,
                "编辑",
                true,
                &[
                    // 保留原生 responder chain，让 WebView 的输入框和 keydown 继续处理快捷键。
                    &Native::undo(app, Some("撤销"))?,
                    &Native::redo(app, Some("重做"))?,
                    &Native::separator(app)?,
                    &Native::cut(app, Some("剪切"))?,
                    &Native::copy(app, Some("拷贝"))?,
                    &Native::paste(app, Some("粘贴"))?,
                    &Native::select_all(app, Some("全选"))?,
                ],
            )?,
            &Submenu::with_items(
                app,
                "窗口",
                true,
                &[
                    &Native::minimize(app, Some("最小化"))?,
                    &Native::maximize(app, Some("缩放"))?,
                    &Native::fullscreen(app, Some("进入全屏幕"))?,
                    &Native::separator(app)?,
                    // performClose: 与红色关闭按钮一样触发 CloseRequested，只隐藏窗口。
                    &Native::close_window(app, Some("关闭窗口"))?,
                ],
            )?,
        ],
    )
}

// AppKit 会额外插入 AutoFill、听写、表情符号和 Close All。只在这两个自定义
// 子菜单打开时移除额外动作，保留原生编辑 responder chain，不写 NSUserDefaults。
mod filter {
    use std::cell::RefCell;

    use objc2::{
        DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
        rc::Retained,
        runtime::{ProtocolObject, Sel},
        sel,
    };
    use objc2_app_kit::{NSApplication, NSMenu, NSMenuDelegate};
    use objc2_foundation::{NSObject, NSObjectProtocol};

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "StickerMenuFilter"]
        #[thread_kind = MainThreadOnly]
        #[ivars = Vec<Sel>]
        struct MenuFilter;

        unsafe impl NSObjectProtocol for MenuFilter {}
        unsafe impl NSMenuDelegate for MenuFilter {
            #[unsafe(method(menuWillOpen:))]
            fn menu_will_open(&self, menu: &NSMenu) {
                for item in menu.itemArray() {
                    if !item.isSeparatorItem()
                        && !item.action().is_some_and(|action| self.ivars().contains(&action))
                    {
                        menu.removeItem(&item);
                    }
                }
                // 系统追加项前后可能还有分隔线。
                let mut previous_separator = true;
                for item in menu.itemArray() {
                    let separator = item.isSeparatorItem();
                    if separator && previous_separator {
                        menu.removeItem(&item);
                    }
                    previous_separator = separator;
                }
                if let Some(last) = menu.itemArray().lastObject()
                    && last.isSeparatorItem()
                {
                    menu.removeItem(&last);
                }
            }
        }
    );

    thread_local! {
        // NSMenu.delegate 是弱引用，且所有菜单操作都在主线程。
        static FILTERS: RefCell<Vec<Retained<MenuFilter>>> = const { RefCell::new(Vec::new()) };
    }

    pub fn install() {
        let mtm = MainThreadMarker::new().expect("菜单在主线程安装");
        let Some(menu) = NSApplication::sharedApplication(mtm).mainMenu() else {
            return;
        };
        for item in menu.itemArray() {
            let Some(submenu) = item.submenu() else {
                continue;
            };
            let actions = match submenu.title().to_string().as_str() {
                "编辑" => vec![
                    sel!(undo:),
                    sel!(redo:),
                    sel!(cut:),
                    sel!(copy:),
                    sel!(paste:),
                    sel!(selectAll:),
                ],
                "窗口" => vec![
                    sel!(performMiniaturize:),
                    sel!(performZoom:),
                    sel!(toggleFullScreen:),
                    sel!(performClose:),
                ],
                _ => continue,
            };
            let delegate = MenuFilter::alloc(mtm).set_ivars(actions);
            let delegate: Retained<MenuFilter> = unsafe { msg_send![super(delegate), init] };
            // 当前 muda 的子菜单 delegate 无回调，仅保留菜单 id；不替换根菜单 delegate，
            // 也不将这个自定义“窗口”菜单设为 NSApp.windowsMenu。
            submenu.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
            FILTERS.with(|filters| filters.borrow_mut().push(delegate));
        }
    }
}

pub fn install_filters() {
    filter::install();
}
