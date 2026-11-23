//! 本机呼出快捷键：键位规则、可回滚注册与录制会话。命令保持同步，锁顺序为 runtime → settings。
use crate::settings::{self, SettingsState};
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

pub const DEFAULT: &str = "Ctrl+Alt+KeyE";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Platform {
    MacOs,
    Windows,
}
impl Platform {
    fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Windows
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum KeyClass {
    Letter,
    Digit,
    Symbol,
    Space,
    Editing,
    Function,
    ExtendedFunction,
}
struct KeySpec {
    code: Code,
    name: &'static str,
    mac: Option<&'static str>,
    win: Option<&'static str>,
    vk: Option<u16>,
    spoken: Option<&'static str>,
    class: KeyClass,
}
const KEYS: &[KeySpec] = &[
    KeySpec {
        code: Code::KeyA,
        name: "KeyA",
        mac: Some("A"),
        win: Some("A"),
        vk: Some(0x41),
        spoken: None,
        class: KeyClass::Letter,
    },
    KeySpec {
        code: Code::KeyB,
        name: "KeyB",
        mac: Some("B"),
        win: Some("B"),
        vk: Some(0x42),
        spoken: None,
        class: KeyClass::Letter,
    },
    KeySpec {
        code: Code::KeyC,
        name: "KeyC",
        mac: Some("C"),
        win: Some("C"),
        vk: Some(0x43),
        spoken: None,
        class: KeyClass::Letter,
    },
    KeySpec {
        code: Code::KeyD,
        name: "KeyD",
        mac: Some("D"),
        win: Some("D"),
        vk: Some(0x44),
        spoken: None,
        class: KeyClass::Letter,
    },
    KeySpec {
        code: Code::KeyE,
        name: "KeyE",
        mac: Some("E"),
        win: Some("E"),
        vk: Some(0x45),
        spoken: None,
        class: KeyClass::Letter,
    },
    KeySpec {
        code: Code::KeyF,
        name: "KeyF",
        mac: Some("F"),
        win: Some("F"),
        vk: Some(0x46),
        spoken: None,
        class: KeyClass::Letter,
    },
    KeySpec {
        code: Code::KeyG,
        name: "KeyG",
        mac: Some("G"),
        win: Some("G"),
        vk: Some(0x47),
        spoken: None,
        class: KeyClass::Letter,
    },
    KeySpec {
        code: Code::KeyH,
        name: "KeyH",
        mac: Some("H"),
        win: Some("H"),
        vk: Some(0x48),
        spoken: None,
        class: KeyClass::Letter,
    },
    KeySpec {
        code: Code::KeyI,
        name: "KeyI",
        mac: Some("I"),
        win: Some("I"),
        vk: Some(0x49),
        spoken: None,
        class: KeyClass::Letter,
    },
    KeySpec {
        code: Code::KeyJ,
        name: "KeyJ",
        mac: Some("J"),
        win: Some("J"),
        vk: Some(0x4A),
        spoken: None,
        class: KeyClass::Letter,
    },
    KeySpec {
        code: Code::KeyK,
        name: "KeyK",
        mac: Some("K"),
        win: Some("K"),
        vk: Some(0x4B),
        spoken: None,
        class: KeyClass::Letter,
    },
    KeySpec {
        code: Code::KeyL,
        name: "KeyL",
        mac: Some("L"),
        win: Some("L"),
        vk: Some(0x4C),
        spoken: None,
        class: KeyClass::Letter,
    },
    KeySpec {
        code: Code::KeyM,
        name: "KeyM",
        mac: Some("M"),
        win: Some("M"),
        vk: Some(0x4D),
        spoken: None,
        class: KeyClass::Letter,
    },
    KeySpec {
        code: Code::KeyN,
        name: "KeyN",
        mac: Some("N"),
        win: Some("N"),
        vk: Some(0x4E),
        spoken: None,
        class: KeyClass::Letter,
    },
    KeySpec {
        code: Code::KeyO,
        name: "KeyO",
        mac: Some("O"),
        win: Some("O"),
        vk: Some(0x4F),
        spoken: None,
        class: KeyClass::Letter,
    },
    KeySpec {
        code: Code::KeyP,
        name: "KeyP",
        mac: Some("P"),
        win: Some("P"),
        vk: Some(0x50),
        spoken: None,
        class: KeyClass::Letter,
    },
    KeySpec {
        code: Code::KeyQ,
        name: "KeyQ",
        mac: Some("Q"),
        win: Some("Q"),
        vk: Some(0x51),
        spoken: None,
        class: KeyClass::Letter,
    },
    KeySpec {
        code: Code::KeyR,
        name: "KeyR",
        mac: Some("R"),
        win: Some("R"),
        vk: Some(0x52),
        spoken: None,
        class: KeyClass::Letter,
    },
    KeySpec {
        code: Code::KeyS,
        name: "KeyS",
        mac: Some("S"),
        win: Some("S"),
        vk: Some(0x53),
        spoken: None,
        class: KeyClass::Letter,
    },
    KeySpec {
        code: Code::KeyT,
        name: "KeyT",
        mac: Some("T"),
        win: Some("T"),
        vk: Some(0x54),
        spoken: None,
        class: KeyClass::Letter,
    },
    KeySpec {
        code: Code::KeyU,
        name: "KeyU",
        mac: Some("U"),
        win: Some("U"),
        vk: Some(0x55),
        spoken: None,
        class: KeyClass::Letter,
    },
    KeySpec {
        code: Code::KeyV,
        name: "KeyV",
        mac: Some("V"),
        win: Some("V"),
        vk: Some(0x56),
        spoken: None,
        class: KeyClass::Letter,
    },
    KeySpec {
        code: Code::KeyW,
        name: "KeyW",
        mac: Some("W"),
        win: Some("W"),
        vk: Some(0x57),
        spoken: None,
        class: KeyClass::Letter,
    },
    KeySpec {
        code: Code::KeyX,
        name: "KeyX",
        mac: Some("X"),
        win: Some("X"),
        vk: Some(0x58),
        spoken: None,
        class: KeyClass::Letter,
    },
    KeySpec {
        code: Code::KeyY,
        name: "KeyY",
        mac: Some("Y"),
        win: Some("Y"),
        vk: Some(0x59),
        spoken: None,
        class: KeyClass::Letter,
    },
    KeySpec {
        code: Code::KeyZ,
        name: "KeyZ",
        mac: Some("Z"),
        win: Some("Z"),
        vk: Some(0x5A),
        spoken: None,
        class: KeyClass::Letter,
    },
    KeySpec {
        code: Code::Digit0,
        name: "Digit0",
        mac: Some("0"),
        win: Some("0"),
        vk: Some(0x30),
        spoken: None,
        class: KeyClass::Digit,
    },
    KeySpec {
        code: Code::Digit1,
        name: "Digit1",
        mac: Some("1"),
        win: Some("1"),
        vk: Some(0x31),
        spoken: None,
        class: KeyClass::Digit,
    },
    KeySpec {
        code: Code::Digit2,
        name: "Digit2",
        mac: Some("2"),
        win: Some("2"),
        vk: Some(0x32),
        spoken: None,
        class: KeyClass::Digit,
    },
    KeySpec {
        code: Code::Digit3,
        name: "Digit3",
        mac: Some("3"),
        win: Some("3"),
        vk: Some(0x33),
        spoken: None,
        class: KeyClass::Digit,
    },
    KeySpec {
        code: Code::Digit4,
        name: "Digit4",
        mac: Some("4"),
        win: Some("4"),
        vk: Some(0x34),
        spoken: None,
        class: KeyClass::Digit,
    },
    KeySpec {
        code: Code::Digit5,
        name: "Digit5",
        mac: Some("5"),
        win: Some("5"),
        vk: Some(0x35),
        spoken: None,
        class: KeyClass::Digit,
    },
    KeySpec {
        code: Code::Digit6,
        name: "Digit6",
        mac: Some("6"),
        win: Some("6"),
        vk: Some(0x36),
        spoken: None,
        class: KeyClass::Digit,
    },
    KeySpec {
        code: Code::Digit7,
        name: "Digit7",
        mac: Some("7"),
        win: Some("7"),
        vk: Some(0x37),
        spoken: None,
        class: KeyClass::Digit,
    },
    KeySpec {
        code: Code::Digit8,
        name: "Digit8",
        mac: Some("8"),
        win: Some("8"),
        vk: Some(0x38),
        spoken: None,
        class: KeyClass::Digit,
    },
    KeySpec {
        code: Code::Digit9,
        name: "Digit9",
        mac: Some("9"),
        win: Some("9"),
        vk: Some(0x39),
        spoken: None,
        class: KeyClass::Digit,
    },
    KeySpec {
        code: Code::Backquote,
        name: "Backquote",
        mac: Some("`"),
        win: Some("`"),
        vk: Some(0xC0),
        spoken: None,
        class: KeyClass::Symbol,
    },
    KeySpec {
        code: Code::Minus,
        name: "Minus",
        mac: Some("-"),
        win: Some("-"),
        vk: Some(0xBD),
        spoken: None,
        class: KeyClass::Symbol,
    },
    KeySpec {
        code: Code::Equal,
        name: "Equal",
        mac: Some("="),
        win: Some("="),
        vk: Some(0xBB),
        spoken: None,
        class: KeyClass::Symbol,
    },
    KeySpec {
        code: Code::BracketLeft,
        name: "BracketLeft",
        mac: Some("["),
        win: Some("["),
        vk: Some(0xDB),
        spoken: None,
        class: KeyClass::Symbol,
    },
    KeySpec {
        code: Code::BracketRight,
        name: "BracketRight",
        mac: Some("]"),
        win: Some("]"),
        vk: Some(0xDD),
        spoken: None,
        class: KeyClass::Symbol,
    },
    KeySpec {
        code: Code::Backslash,
        name: "Backslash",
        mac: Some("\\"),
        win: Some("\\"),
        vk: Some(0xDC),
        spoken: None,
        class: KeyClass::Symbol,
    },
    KeySpec {
        code: Code::Semicolon,
        name: "Semicolon",
        mac: Some(";"),
        win: Some(";"),
        vk: Some(0xBA),
        spoken: None,
        class: KeyClass::Symbol,
    },
    KeySpec {
        code: Code::Quote,
        name: "Quote",
        mac: Some("'"),
        win: Some("'"),
        vk: Some(0xDE),
        spoken: None,
        class: KeyClass::Symbol,
    },
    KeySpec {
        code: Code::Comma,
        name: "Comma",
        mac: Some(","),
        win: Some(","),
        vk: Some(0xBC),
        spoken: None,
        class: KeyClass::Symbol,
    },
    KeySpec {
        code: Code::Period,
        name: "Period",
        mac: Some("."),
        win: Some("."),
        vk: Some(0xBE),
        spoken: None,
        class: KeyClass::Symbol,
    },
    KeySpec {
        code: Code::Slash,
        name: "Slash",
        mac: Some("/"),
        win: Some("/"),
        vk: Some(0xBF),
        spoken: None,
        class: KeyClass::Symbol,
    },
    KeySpec {
        code: Code::Space,
        name: "Space",
        mac: Some("Space"),
        win: Some("Space"),
        vk: Some(0x20),
        spoken: Some("空格"),
        class: KeyClass::Space,
    },
    KeySpec {
        code: Code::Enter,
        name: "Enter",
        mac: Some("↩"),
        win: Some("Enter"),
        vk: Some(0x0D),
        spoken: Some("回车"),
        class: KeyClass::Editing,
    },
    KeySpec {
        code: Code::Backspace,
        name: "Backspace",
        mac: Some("⌫"),
        win: Some("Backspace"),
        vk: Some(0x08),
        spoken: Some("退格"),
        class: KeyClass::Editing,
    },
    KeySpec {
        code: Code::Delete,
        name: "Delete",
        mac: Some("⌦"),
        win: Some("Delete"),
        vk: Some(0x2E),
        spoken: Some("Delete"),
        class: KeyClass::Editing,
    },
    KeySpec {
        code: Code::Home,
        name: "Home",
        mac: Some("Home"),
        win: Some("Home"),
        vk: Some(0x24),
        spoken: None,
        class: KeyClass::Editing,
    },
    KeySpec {
        code: Code::End,
        name: "End",
        mac: Some("End"),
        win: Some("End"),
        vk: Some(0x23),
        spoken: None,
        class: KeyClass::Editing,
    },
    KeySpec {
        code: Code::PageUp,
        name: "PageUp",
        mac: Some("PageUp"),
        win: Some("PageUp"),
        vk: Some(0x21),
        spoken: None,
        class: KeyClass::Editing,
    },
    KeySpec {
        code: Code::PageDown,
        name: "PageDown",
        mac: Some("PageDown"),
        win: Some("PageDown"),
        vk: Some(0x22),
        spoken: None,
        class: KeyClass::Editing,
    },
    KeySpec {
        code: Code::ArrowUp,
        name: "ArrowUp",
        mac: Some("↑"),
        win: Some("↑"),
        vk: Some(0x26),
        spoken: Some("上箭头"),
        class: KeyClass::Editing,
    },
    KeySpec {
        code: Code::ArrowDown,
        name: "ArrowDown",
        mac: Some("↓"),
        win: Some("↓"),
        vk: Some(0x28),
        spoken: Some("下箭头"),
        class: KeyClass::Editing,
    },
    KeySpec {
        code: Code::ArrowLeft,
        name: "ArrowLeft",
        mac: Some("←"),
        win: Some("←"),
        vk: Some(0x25),
        spoken: Some("左箭头"),
        class: KeyClass::Editing,
    },
    KeySpec {
        code: Code::ArrowRight,
        name: "ArrowRight",
        mac: Some("→"),
        win: Some("→"),
        vk: Some(0x27),
        spoken: Some("右箭头"),
        class: KeyClass::Editing,
    },
    KeySpec {
        code: Code::Insert,
        name: "Insert",
        mac: None,
        win: Some("Insert"),
        vk: Some(0x2D),
        spoken: None,
        class: KeyClass::Editing,
    },
    KeySpec {
        code: Code::F1,
        name: "F1",
        mac: Some("F1"),
        win: Some("F1"),
        vk: Some(0x70),
        spoken: None,
        class: KeyClass::Function,
    },
    KeySpec {
        code: Code::F2,
        name: "F2",
        mac: Some("F2"),
        win: Some("F2"),
        vk: Some(0x71),
        spoken: None,
        class: KeyClass::Function,
    },
    KeySpec {
        code: Code::F3,
        name: "F3",
        mac: Some("F3"),
        win: Some("F3"),
        vk: Some(0x72),
        spoken: None,
        class: KeyClass::Function,
    },
    KeySpec {
        code: Code::F4,
        name: "F4",
        mac: Some("F4"),
        win: Some("F4"),
        vk: Some(0x73),
        spoken: None,
        class: KeyClass::Function,
    },
    KeySpec {
        code: Code::F5,
        name: "F5",
        mac: Some("F5"),
        win: Some("F5"),
        vk: Some(0x74),
        spoken: None,
        class: KeyClass::Function,
    },
    KeySpec {
        code: Code::F6,
        name: "F6",
        mac: Some("F6"),
        win: Some("F6"),
        vk: Some(0x75),
        spoken: None,
        class: KeyClass::Function,
    },
    KeySpec {
        code: Code::F7,
        name: "F7",
        mac: Some("F7"),
        win: Some("F7"),
        vk: Some(0x76),
        spoken: None,
        class: KeyClass::Function,
    },
    KeySpec {
        code: Code::F8,
        name: "F8",
        mac: Some("F8"),
        win: Some("F8"),
        vk: Some(0x77),
        spoken: None,
        class: KeyClass::Function,
    },
    KeySpec {
        code: Code::F9,
        name: "F9",
        mac: Some("F9"),
        win: Some("F9"),
        vk: Some(0x78),
        spoken: None,
        class: KeyClass::Function,
    },
    KeySpec {
        code: Code::F10,
        name: "F10",
        mac: Some("F10"),
        win: Some("F10"),
        vk: Some(0x79),
        spoken: None,
        class: KeyClass::Function,
    },
    KeySpec {
        code: Code::F11,
        name: "F11",
        mac: Some("F11"),
        win: Some("F11"),
        vk: Some(0x7A),
        spoken: None,
        class: KeyClass::Function,
    },
    KeySpec {
        code: Code::F12,
        name: "F12",
        mac: Some("F12"),
        win: Some("F12"),
        vk: Some(0x7B),
        spoken: None,
        class: KeyClass::Function,
    },
    KeySpec {
        code: Code::F13,
        name: "F13",
        mac: Some("F13"),
        win: Some("F13"),
        vk: Some(0x7C),
        spoken: None,
        class: KeyClass::ExtendedFunction,
    },
    KeySpec {
        code: Code::F14,
        name: "F14",
        mac: Some("F14"),
        win: Some("F14"),
        vk: Some(0x7D),
        spoken: None,
        class: KeyClass::ExtendedFunction,
    },
    KeySpec {
        code: Code::F15,
        name: "F15",
        mac: Some("F15"),
        win: Some("F15"),
        vk: Some(0x7E),
        spoken: None,
        class: KeyClass::ExtendedFunction,
    },
    KeySpec {
        code: Code::F16,
        name: "F16",
        mac: Some("F16"),
        win: Some("F16"),
        vk: Some(0x7F),
        spoken: None,
        class: KeyClass::ExtendedFunction,
    },
    KeySpec {
        code: Code::F17,
        name: "F17",
        mac: Some("F17"),
        win: Some("F17"),
        vk: Some(0x80),
        spoken: None,
        class: KeyClass::ExtendedFunction,
    },
    KeySpec {
        code: Code::F18,
        name: "F18",
        mac: Some("F18"),
        win: Some("F18"),
        vk: Some(0x81),
        spoken: None,
        class: KeyClass::ExtendedFunction,
    },
    KeySpec {
        code: Code::F19,
        name: "F19",
        mac: Some("F19"),
        win: Some("F19"),
        vk: Some(0x82),
        spoken: None,
        class: KeyClass::ExtendedFunction,
    },
    KeySpec {
        code: Code::F20,
        name: "F20",
        mac: Some("F20"),
        win: Some("F20"),
        vk: Some(0x83),
        spoken: None,
        class: KeyClass::ExtendedFunction,
    },
    KeySpec {
        code: Code::F21,
        name: "F21",
        mac: None,
        win: Some("F21"),
        vk: Some(0x84),
        spoken: None,
        class: KeyClass::ExtendedFunction,
    },
    KeySpec {
        code: Code::F22,
        name: "F22",
        mac: None,
        win: Some("F22"),
        vk: Some(0x85),
        spoken: None,
        class: KeyClass::ExtendedFunction,
    },
    KeySpec {
        code: Code::F23,
        name: "F23",
        mac: None,
        win: Some("F23"),
        vk: Some(0x86),
        spoken: None,
        class: KeyClass::ExtendedFunction,
    },
    KeySpec {
        code: Code::F24,
        name: "F24",
        mac: None,
        win: Some("F24"),
        vk: Some(0x87),
        spoken: None,
        class: KeyClass::ExtendedFunction,
    },
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PanelShortcut {
    modifiers: Modifiers,
    code: Code,
}
fn modifier(text: &str) -> Option<Modifiers> {
    match text.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Some(Modifiers::CONTROL),
        "alt" | "option" | "opt" => Some(Modifiers::ALT),
        "shift" => Some(Modifiers::SHIFT),
        "super" | "cmd" | "command" | "meta" | "win" => Some(Modifiers::SUPER),
        _ => None,
    }
}
fn excluded(code: &str) -> String {
    let lower = code.to_ascii_lowercase();
    match lower.as_str() {
        "escape" => "Esc 用来取消录制，不能设为快捷键",
        "tab" => "Tab 用来切换焦点，不能设为快捷键",
        "capslock" | "numlock" | "scrolllock" => "锁定键不能设为快捷键",
        "printscreen" => "Print Screen 留给截图，不能设为快捷键",
        "controlleft" | "controlright" | "altleft" | "altright" | "shiftleft" | "shiftright"
        | "metaleft" | "metaright" | "osleft" | "osright" | "fn" | "fnlock" => {
            "请在按住修饰键的同时再按一个键"
        }
        _ if modifier(code).is_some() => "请在按住修饰键的同时再按一个键",
        _ if lower.starts_with("numpad") => "暂不支持小键盘按键",
        _ if lower.starts_with("audio")
            || lower.starts_with("media")
            || lower.starts_with("volume") =>
        {
            "媒体键不能设为快捷键"
        }
        _ => "暂不支持这个按键",
    }
    .into()
}
fn key_by_name(name: &str) -> Result<&'static KeySpec, String> {
    KEYS.iter()
        .find(|k| k.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| excluded(name))
}
impl PanelShortcut {
    fn parse(text: &str) -> Result<Self, String> {
        if text.len() > 64 {
            return Err("快捷键过长".into());
        }
        let parts: Vec<_> = text.split('+').map(str::trim).collect();
        if parts.iter().any(|p| p.is_empty()) {
            return Err("快捷键不能包含空段".into());
        }
        let mut modifiers = Modifiers::empty();
        for part in &parts[..parts.len() - 1] {
            let m = modifier(part).ok_or_else(|| excluded(part))?;
            if modifiers.contains(m) {
                return Err("修饰键不能重复".into());
            }
            modifiers |= m;
        }
        let last = parts.last().unwrap();
        let name = if last.len() == 1 && last.as_bytes()[0].is_ascii_alphabetic() {
            format!("Key{}", last.to_ascii_uppercase())
        } else if last.len() == 1 && last.as_bytes()[0].is_ascii_digit() {
            format!("Digit{last}")
        } else {
            last.to_string()
        };
        Ok(Self {
            modifiers,
            code: key_by_name(&name)?.code,
        })
    }
    fn spec(self) -> &'static KeySpec {
        KEYS.iter().find(|k| k.code == self.code).unwrap()
    }
    fn canonical(self) -> String {
        let mut parts = Vec::new();
        for (m, n) in [
            (Modifiers::CONTROL, "Ctrl"),
            (Modifiers::ALT, "Alt"),
            (Modifiers::SHIFT, "Shift"),
            (Modifiers::SUPER, "Super"),
        ] {
            if self.modifiers.contains(m) {
                parts.push(n);
            }
        }
        parts.push(self.spec().name);
        parts.join("+")
    }
    fn label_for(self, platform: Platform) -> String {
        let mac = platform == Platform::MacOs;
        let names = if mac {
            ["⌃", "⌥", "⇧", "⌘"]
        } else {
            ["Ctrl", "Alt", "Shift", "Win"]
        };
        let mut parts = self.modifier_labels(names);
        parts.push(
            if mac {
                self.spec().mac
            } else {
                self.spec().win
            }
            .unwrap_or(self.spec().name),
        );
        parts.join(if mac { "" } else { "+" })
    }
    fn modifier_labels(self, names: [&'static str; 4]) -> Vec<&'static str> {
        [
            Modifiers::CONTROL,
            Modifiers::ALT,
            Modifiers::SHIFT,
            Modifiers::SUPER,
        ]
        .into_iter()
        .zip(names)
        .filter_map(|(m, n)| self.modifiers.contains(m).then_some(n))
        .collect()
    }
    fn accessible_label_for(self, platform: Platform) -> String {
        let mut parts = self.modifier_labels(if platform == Platform::MacOs {
            ["Control", "Option", "Shift", "Command"]
        } else {
            ["Ctrl", "Alt", "Shift", "Win"]
        });
        parts.push(match (platform, self.code) {
            (Platform::MacOs, Code::Backspace) => "删除",
            (Platform::MacOs, Code::Delete) => "向前删除",
            _ => self
                .spec()
                .spoken
                .or(self.spec().win)
                .unwrap_or(self.spec().name),
        });
        parts.join("+")
    }
    fn label(self) -> String {
        self.label_for(Platform::current())
    }
    fn validate(self) -> Result<(), String> {
        self.validate_for(Platform::current())
    }
    fn validate_for(self, platform: Platform) -> Result<(), String> {
        let k = self.spec();
        let mac = platform == Platform::MacOs;
        let label = self.label_for(platform);
        if (if mac { k.mac } else { k.win }).is_none() {
            return Err(format!("这台电脑上不能使用 {}", k.name));
        }
        if !mac && self.modifiers.contains(Modifiers::SUPER) {
            return Err("Windows 徽标键的组合保留给系统，请改用 Ctrl 和 Alt".into());
        }
        let count = [Modifiers::CONTROL, Modifiers::ALT, Modifiers::SUPER]
            .into_iter()
            .filter(|m| self.modifiers.contains(*m))
            .count();
        if k.class != KeyClass::ExtendedFunction {
            if count == 0 {
                return Err(format!(
                    "{label} 缺少修饰键，请搭配 {} 使用",
                    if mac {
                        "⌃、⌥ 或 ⌘"
                    } else {
                        "Ctrl 或 Alt"
                    }
                ));
            }
            if count == 1 && !matches!(k.class, KeyClass::Function | KeyClass::Space) {
                return Err(format!(
                    "{label} 容易和打字或应用里的快捷键冲突，请{}",
                    if mac {
                        "至少按住 ⌃、⌥、⌘ 中的两个，例如 ⌃⌥E"
                    } else {
                        "同时按住 Ctrl 和 Alt，例如 Ctrl+Alt+E"
                    }
                ));
            }
        }
        let canonical = self.canonical();
        if let Some((_, purpose)) = (if mac { MAC_RESERVED } else { WIN_RESERVED })
            .iter()
            .find(|(key, _)| *key == canonical)
        {
            return Err(format!("{label} 已用于{purpose}，请换一个组合"));
        }
        Ok(())
    }
    fn from_key_input(input: &KeyInput, platform: Platform) -> Result<Self, String> {
        if input.key_code == 229 {
            return Err("请先切换到英文输入状态再录制".into());
        }
        let key = if platform == Platform::Windows && input.key_code != 0 {
            if matches!(input.key_code,16..=18 | 91 | 92 | 160..=165) {
                return Err(excluded("ControlLeft"));
            }
            KEYS.iter()
                .find(|k| k.vk.map(u32::from) == Some(input.key_code))
                .ok_or_else(|| excluded(&input.code))?
        } else {
            key_by_name(&input.code)?
        };
        let mut modifiers = Modifiers::empty();
        for (yes, m) in [
            (input.ctrl, Modifiers::CONTROL),
            (input.alt, Modifiers::ALT),
            (input.shift, Modifiers::SHIFT),
            (input.meta, Modifiers::SUPER),
        ] {
            if yes {
                modifiers |= m;
            }
        }
        Ok(Self {
            modifiers,
            code: key.code,
        })
    }
    fn hotkey(self) -> Shortcut {
        Shortcut::new(Some(self.modifiers), self.code)
    }
}
pub fn normalize_stored(text: &str) -> Result<String, String> {
    let shortcut = PanelShortcut::parse(text)?;
    shortcut.validate()?;
    Ok(shortcut.canonical())
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyInput {
    pub code: String,
    pub key_code: u32,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}
const MAC_RESERVED: &[(&str, &str)] = &[
    ("Ctrl+Space", "切换输入法"),
    ("Super+Space", "聚焦搜索"),
    ("Ctrl+Alt+Space", "切换到下一个输入法"),
    ("Ctrl+Super+Space", "表情与符号"),
    ("Alt+Super+Space", "访达搜索窗口"),
    ("Ctrl+Super+KeyQ", "锁定屏幕"),
    ("Ctrl+Super+KeyF", "进入全屏幕"),
    ("Ctrl+Super+KeyD", "查询词典"),
    ("Alt+Super+KeyD", "显示或隐藏程序坞"),
    ("Alt+Super+KeyH", "隐藏其他应用"),
    ("Alt+Super+KeyM", "最小化全部窗口"),
    ("Alt+Super+KeyW", "关闭全部窗口"),
    ("Ctrl+Shift+Super+Digit3", "截屏到剪贴板"),
    ("Ctrl+Shift+Super+Digit4", "截取所选部分到剪贴板"),
    ("Alt+Shift+Super+KeyQ", "立即退出登录"),
    ("Alt+Super+Digit8", "辅助功能缩放"),
    ("Alt+Super+Equal", "辅助功能放大"),
    ("Alt+Super+Minus", "辅助功能缩小"),
    ("Ctrl+Alt+Super+Digit8", "反转颜色"),
    ("Alt+Super+F5", "辅助功能快捷键面板"),
    ("Super+F5", "旁白"),
    ("Ctrl+F1", "全键盘控制"),
    ("Ctrl+F2", "菜单栏导航"),
    ("Ctrl+F3", "程序坞导航"),
    ("Ctrl+F4", "窗口导航"),
    ("Ctrl+F5", "工具栏导航"),
    ("Ctrl+F6", "浮动窗口导航"),
    ("Ctrl+F7", "切换控制方式"),
    ("Ctrl+F8", "状态菜单导航"),
    ("F14", "调节屏幕亮度"),
    ("F15", "调节屏幕亮度"),
];
const WIN_RESERVED: &[(&str, &str)] = &[
    ("Ctrl+Alt+Delete", "安全选项"),
    ("Ctrl+Space", "切换输入法中英文"),
    ("Alt+Space", "窗口菜单"),
    ("Alt+F4", "关闭窗口"),
    ("Ctrl+F4", "关闭标签页或文档"),
];
pub(crate) trait Registrar {
    fn register(&self, s: PanelShortcut) -> Result<(), String>;
    fn unregister(&self, s: PanelShortcut) -> Result<(), String>;
}
#[derive(Default)]
struct ShortcutMachine {
    registered: Option<PanelShortcut>,
    error: Option<String>,
    recording: Option<u64>,
    next_recording: u64,
    revision: u64,
}
impl ShortcutMachine {
    fn startup(&mut self, r: &impl Registrar, configured: Option<PanelShortcut>) {
        self.revision += 1;
        if let Some(s) = configured {
            match r.register(s) {
                Ok(()) => {
                    self.registered = Some(s);
                    self.error = None;
                }
                Err(reason) => self.failed(s, &reason),
            }
        }
    }
    fn failed(&mut self, s: PanelShortcut, reason: &str) {
        self.error = Some(format!("呼出快捷键 {} 无法使用：{reason}", s.label()));
    }
    fn apply(
        &mut self,
        r: &impl Registrar,
        configured: Option<PanelShortcut>,
        requested: Option<PanelShortcut>,
        persist: impl FnOnce(Option<String>) -> Result<(), String>,
    ) -> Result<(), String> {
        if let Some(s) = requested {
            s.validate()?;
        }
        self.revision += 1;
        let recording = self.recording.is_some();
        let before = self.registered;
        if self.registered != requested {
            if let Some(old) = self.registered {
                r.unregister(old)
                    .map_err(|e| format!("无法停用原来的快捷键 {}：{e}", old.label()))?;
                self.registered = None;
            }
            if let Some(s) = requested {
                if let Err(reason) = r.register(s) {
                    if !recording {
                        if let Some(old) = before {
                            match r.register(old) {
                                Ok(()) => self.registered = Some(old),
                                Err(reason2) => {
                                    self.failed(old, &reason2);
                                    return Err(format!(
                                        "{} 无法使用：{reason}；原来的快捷键 {} 也没能恢复：{reason2}",
                                        s.label(),
                                        old.label()
                                    ));
                                }
                            }
                        } else if requested == configured {
                            self.failed(s, &reason);
                        }
                    }
                    return Err(format!("{} 无法使用：{reason}", s.label()));
                }
                self.registered = Some(s);
            }
        }
        if requested != configured
            && let Err(e) = persist(requested.map(PanelShortcut::canonical))
        {
            let mut rollback_errors = Vec::new();
            if let Some(s) = self.registered {
                match r.unregister(s) {
                    Ok(()) => self.registered = None,
                    Err(reason) => {
                        rollback_errors.push(format!("无法停用 {}：{reason}", s.label()))
                    }
                }
            }
            if !recording && let Some(old) = before {
                if self.registered.is_none() {
                    match r.register(old) {
                        Ok(()) => self.registered = Some(old),
                        Err(reason) => {
                            self.failed(old, &reason);
                            rollback_errors.push(reason);
                        }
                    }
                } else {
                    self.failed(old, "回滚未完成");
                }
            }
            if !rollback_errors.is_empty() {
                self.error = Some(format!("快捷键回滚失败：{}", rollback_errors.join("；")));
            }
            return Err(format!(
                "无法保存快捷键设置：{e}{}",
                if rollback_errors.is_empty() {
                    String::new()
                } else {
                    format!("；{}", rollback_errors.join("；"))
                }
            ));
        }
        self.error = None;
        self.recording = None;
        Ok(())
    }
    fn begin_recording(&mut self, r: &impl Registrar) -> Result<u64, String> {
        if let Some(s) = self.registered {
            r.unregister(s)
                .map_err(|e| format!("无法暂停当前快捷键 {}：{e}", s.label()))?;
            self.registered = None;
        }
        self.next_recording += 1;
        let id = self.next_recording;
        self.recording = Some(id);
        self.revision += 1;
        Ok(id)
    }
    fn active_hotkey_id(&self, configured: Option<PanelShortcut>) -> Option<u32> {
        if self.recording.is_none() && self.registered == configured {
            configured.map(|s| s.hotkey().id())
        } else {
            None
        }
    }
    fn end_recording(
        &mut self,
        r: &impl Registrar,
        id: Option<u64>,
        configured: Option<PanelShortcut>,
    ) -> bool {
        if self.recording.is_none() || id.is_some_and(|id| self.recording != Some(id)) {
            return false;
        }
        self.recording = None;
        self.revision += 1;
        if self.registered != configured {
            if let Some(s) = self.registered {
                if let Err(e) = r.unregister(s) {
                    self.failed(s, &e);
                    return true;
                }
                self.registered = None;
            }
            if let Some(s) = configured {
                match r.register(s) {
                    Ok(()) => self.registered = Some(s),
                    Err(e) => {
                        self.failed(s, &e);
                        return true;
                    }
                }
            }
        }
        self.error = None;
        true
    }
    fn snapshot(&self, configured: Option<PanelShortcut>) -> PanelShortcutState {
        let default = PanelShortcut::parse(DEFAULT).unwrap();
        PanelShortcutState {
            shortcut: configured.map(PanelShortcut::canonical),
            label: configured.map(PanelShortcut::label),
            accessible_label: configured.map(|s| s.accessible_label_for(Platform::current())),
            default_shortcut: DEFAULT.into(),
            default_label: default.label(),
            status: if self.error.is_some() {
                ShortcutStatus::Failed
            } else if configured.is_none() {
                ShortcutStatus::Disabled
            } else {
                ShortcutStatus::Active
            },
            error: self.error.clone(),
            recording_id: self.recording,
            revision: self.revision,
        }
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ShortcutStatus {
    Active,
    Disabled,
    Failed,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelShortcutState {
    shortcut: Option<String>,
    label: Option<String>,
    accessible_label: Option<String>,
    default_shortcut: String,
    default_label: String,
    status: ShortcutStatus,
    error: Option<String>,
    recording_id: Option<u64>,
    revision: u64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutCandidate {
    shortcut: Option<String>,
    label: String,
    accessible_label: String,
    error: Option<String>,
}
const NO_ACTIVE_HOTKEY: u64 = u64::MAX;
const RECORDING_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Default)]
struct WatchdogState {
    started: bool,
    deadline: Option<(u64, Instant)>,
}
struct Watchdog {
    state: Arc<(Mutex<WatchdogState>, Condvar)>,
}
impl Default for Watchdog {
    fn default() -> Self {
        Self {
            state: Arc::new((Mutex::new(WatchdogState::default()), Condvar::new())),
        }
    }
}
impl Watchdog {
    fn arm(&self, app: AppHandle, id: u64, timeout: Duration) {
        let (lock, cvar) = &*self.state;
        let mut state = lock.lock().unwrap();
        state.deadline = Some((id, Instant::now() + timeout));
        let should_spawn = !state.started;
        if should_spawn {
            state.started = true;
        }
        cvar.notify_one();
        drop(state);
        if should_spawn {
            let shared = Arc::clone(&self.state);
            std::thread::spawn(move || {
                let (lock, cvar) = &*shared;
                let mut state = lock.lock().unwrap();
                loop {
                    let Some((id, deadline)) = state.deadline else {
                        state = cvar.wait(state).unwrap();
                        continue;
                    };
                    if let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
                        let (guard, _) = cvar.wait_timeout(state, remaining).unwrap();
                        state = guard;
                        continue;
                    }
                    state.deadline = None;
                    drop(state);
                    let handle = app.clone();
                    let _ = app.run_on_main_thread(move || {
                        end_panel_shortcut_recording(handle, Some(id));
                    });
                    state = lock.lock().unwrap();
                }
            });
        }
    }
    fn disarm(&self) {
        let (lock, cvar) = &*self.state;
        let mut state = lock.lock().unwrap();
        if state.deadline.take().is_some() {
            cvar.notify_one();
        }
    }
}
struct ShortcutRuntime {
    machine: Mutex<ShortcutMachine>,
    recording: AtomicBool,
    active_hotkey: AtomicU64,
    watchdog: Watchdog,
}
impl Default for ShortcutRuntime {
    fn default() -> Self {
        Self {
            machine: Mutex::new(ShortcutMachine::default()),
            recording: AtomicBool::new(false),
            active_hotkey: AtomicU64::new(NO_ACTIVE_HOTKEY),
            watchdog: Watchdog::default(),
        }
    }
}
impl ShortcutRuntime {
    fn sync_flags(&self, machine: &ShortcutMachine, configured: Option<PanelShortcut>) {
        self.recording
            .store(machine.recording.is_some(), Ordering::SeqCst);
        let active = machine
            .active_hotkey_id(configured)
            .map_or(NO_ACTIVE_HOTKEY, u64::from);
        self.active_hotkey.store(active, Ordering::SeqCst);
    }
}
struct AppRegistrar<'a>(&'a AppHandle);
fn describe_os_error(error: impl std::fmt::Display) -> String {
    let raw = error.to_string();
    if raw.to_ascii_lowercase().contains("already registered") {
        "已被其他应用或系统占用".into()
    } else {
        format!("系统拒绝注册（{raw}）")
    }
}
impl Registrar for AppRegistrar<'_> {
    fn register(&self, s: PanelShortcut) -> Result<(), String> {
        let hotkey = s.hotkey();
        if self.0.global_shortcut().is_registered(hotkey) {
            return Ok(());
        }
        let expected_id = u64::from(hotkey.id());
        self.0
            .global_shortcut()
            .on_shortcut(hotkey, move |app, _, event| {
                if event.state != ShortcutState::Pressed
                    || app
                        .state::<ShortcutRuntime>()
                        .active_hotkey
                        .load(Ordering::SeqCst)
                        != expected_id
                {
                    return;
                }
                let handle = app.clone();
                let _ = app.run_on_main_thread(move || {
                    if handle
                        .state::<ShortcutRuntime>()
                        .active_hotkey
                        .load(Ordering::SeqCst)
                        == expected_id
                        && let Err(e) = crate::panel::toggle(&handle)
                    {
                        eprintln!("无法切换表情面板：{e}");
                    }
                });
            })
            .map_err(describe_os_error)
    }
    fn unregister(&self, s: PanelShortcut) -> Result<(), String> {
        if !self.0.global_shortcut().is_registered(s.hotkey()) {
            return Ok(());
        }
        self.0
            .global_shortcut()
            .unregister(s.hotkey())
            .map_err(describe_os_error)
    }
}
fn configured(app: &AppHandle) -> Option<PanelShortcut> {
    app.state::<SettingsState>()
        .get()
        .panel_shortcut
        .as_deref()
        .map(|s| PanelShortcut::parse(s).expect("normalized settings"))
}
fn emit(app: &AppHandle, state: &PanelShortcutState) {
    if let Err(e) = app.emit("panel-shortcut-changed", state) {
        eprintln!("无法广播快捷键状态：{e}");
    }
}
pub fn init(app: &AppHandle) {
    app.manage(ShortcutRuntime::default());
    let runtime = app.state::<ShortcutRuntime>();
    let mut m = runtime.machine.lock().unwrap();
    let current = configured(app);
    m.startup(&AppRegistrar(app), current);
    runtime.sync_flags(&m, current);
}
#[tauri::command]
pub fn panel_shortcut_state(app: AppHandle) -> PanelShortcutState {
    let runtime = app.state::<ShortcutRuntime>();
    let m = runtime.machine.lock().unwrap();
    m.snapshot(configured(&app))
}
#[tauri::command]
pub fn check_panel_shortcut(input: KeyInput) -> ShortcutCandidate {
    match PanelShortcut::from_key_input(&input, Platform::current()) {
        Ok(s) => ShortcutCandidate {
            shortcut: Some(s.canonical()),
            label: s.label(),
            accessible_label: s.accessible_label_for(Platform::current()),
            error: s.validate().err(),
        },
        Err(error) => ShortcutCandidate {
            shortcut: None,
            label: String::new(),
            accessible_label: String::new(),
            error: Some(error),
        },
    }
}
#[tauri::command]
pub fn set_panel_shortcut(
    app: AppHandle,
    shortcut: Option<String>,
) -> Result<PanelShortcutState, String> {
    let requested = shortcut.as_deref().map(PanelShortcut::parse).transpose()?;
    let runtime = app.state::<ShortcutRuntime>();
    let mut m = runtime.machine.lock().unwrap();
    let before = m.revision;
    // settings-changed 同样在 runtime 锁外广播，并读取最新快照避免覆盖并发修改。
    let mut saved = false;
    let result = m.apply(&AppRegistrar(&app), configured(&app), requested, |value| {
        settings::update_without_event(&app, |s| s.panel_shortcut = value)?;
        saved = true;
        Ok(())
    });
    let current = configured(&app);
    runtime.sync_flags(&m, current);
    let recording = m.recording.is_some();
    let state = m.snapshot(current);
    let changed = before != m.revision;
    drop(m);
    if !recording {
        runtime.watchdog.disarm();
    }
    if saved && let Err(e) = app.emit("settings-changed", app.state::<SettingsState>().get()) {
        eprintln!("快捷键设置已保存，但广播失败：{e}");
    }
    if changed {
        emit(&app, &state);
    }
    result.map(|()| state)
}
#[tauri::command]
pub fn begin_panel_shortcut_recording(app: AppHandle) -> Result<PanelShortcutState, String> {
    let runtime = app.state::<ShortcutRuntime>();
    let mut m = runtime.machine.lock().unwrap();
    let id = m.begin_recording(&AppRegistrar(&app))?;
    let current = configured(&app);
    runtime.sync_flags(&m, current);
    let state = m.snapshot(current);
    drop(m);
    runtime.watchdog.arm(app.clone(), id, RECORDING_TIMEOUT);
    emit(&app, &state);
    Ok(state)
}
#[tauri::command]
pub fn end_panel_shortcut_recording(
    app: AppHandle,
    recording_id: Option<u64>,
) -> PanelShortcutState {
    let runtime = app.state::<ShortcutRuntime>();
    let mut m = runtime.machine.lock().unwrap();
    let current = configured(&app);
    let changed = m.end_recording(&AppRegistrar(&app), recording_id, current);
    runtime.sync_flags(&m, current);
    let recording = m.recording.is_some();
    let state = m.snapshot(current);
    drop(m);
    if !recording {
        runtime.watchdog.disarm();
    }
    if changed {
        emit(&app, &state);
    }
    state
}
pub fn cancel_recording(app: &AppHandle) {
    if app
        .try_state::<ShortcutRuntime>()
        .is_some_and(|runtime| runtime.recording.load(Ordering::SeqCst))
    {
        end_panel_shortcut_recording(app.clone(), None);
    }
}
pub fn on_window_event(window: &tauri::Window, event: &WindowEvent) {
    if window.label() == "main"
        && matches!(
            event,
            WindowEvent::Focused(false)
                | WindowEvent::CloseRequested { .. }
                | WindowEvent::Destroyed
        )
    {
        cancel_recording(window.app_handle());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    fn s(text: &str) -> PanelShortcut {
        PanelShortcut::parse(text).unwrap()
    }
    fn old() -> Option<PanelShortcut> {
        Some(s(DEFAULT))
    }
    fn new() -> Option<PanelShortcut> {
        Some(s("Ctrl+Alt+KeyK"))
    }
    #[derive(Default)]
    struct Fake {
        calls: RefCell<Vec<String>>,
        results: RefCell<VecDeque<Result<(), String>>>,
    }
    impl Fake {
        fn respond(&self, values: &[Result<(), &str>]) {
            self.results
                .borrow_mut()
                .extend(values.iter().map(|v| v.map_err(str::to_string)));
        }
        fn call(&self, op: &str, key: PanelShortcut) -> Result<(), String> {
            self.calls
                .borrow_mut()
                .push(format!("{op} {}", key.canonical()));
            self.results.borrow_mut().pop_front().unwrap_or(Ok(()))
        }
    }
    impl Registrar for Fake {
        fn register(&self, s: PanelShortcut) -> Result<(), String> {
            self.call("register", s)
        }
        fn unregister(&self, s: PanelShortcut) -> Result<(), String> {
            self.call("unregister", s)
        }
    }
    fn started() -> (ShortcutMachine, Fake) {
        let mut m = ShortcutMachine::default();
        let r = Fake::default();
        m.startup(&r, old());
        r.calls.borrow_mut().clear();
        (m, r)
    }
    #[test]
    fn parsing_and_canonical_order() {
        for (input, expected) in [
            (" option + control + e ", DEFAULT),
            ("WIN+shift+ALT+CTRL+k", "Ctrl+Alt+Shift+Super+KeyK"),
            ("cmd+opt+7", "Alt+Super+Digit7"),
            ("control+alt+bracketleft", "Ctrl+Alt+BracketLeft"),
        ] {
            assert_eq!(s(input).canonical(), expected);
        }
        for input in [
            "",
            "Ctrl++E",
            "Ctrl+control+E",
            "Ctrl+Alt",
            "KeyA+KeyB",
            "Ctrl+Alt+Tab",
            "Ctrl+Alt+Numpad1",
        ] {
            assert!(PanelShortcut::parse(input).is_err(), "{input}");
        }
        assert!(PanelShortcut::parse(&"x".repeat(65)).is_err());
        assert_eq!(normalize_stored("alt+ctrl+e").unwrap(), DEFAULT);
        for key in KEYS {
            assert_eq!(s(key.name).code, key.code);
        }
    }
    #[test]
    fn labels_and_accessibility() {
        let key = s("Ctrl+Alt+Shift+Super+KeyK");
        assert_eq!(key.label_for(Platform::MacOs), "⌃⌥⇧⌘K");
        assert_eq!(key.label_for(Platform::Windows), "Ctrl+Alt+Shift+Win+K");
        assert_eq!(
            key.accessible_label_for(Platform::MacOs),
            "Control+Option+Shift+Command+K"
        );
        for (name, mac, win, spoken) in [
            ("Space", "Space", "Space", "空格"),
            ("Enter", "↩", "Enter", "回车"),
            ("ArrowUp", "↑", "↑", "上箭头"),
        ] {
            let key = s(&format!("Ctrl+Alt+{name}"));
            assert_eq!(key.label_for(Platform::MacOs), format!("⌃⌥{mac}"));
            assert_eq!(key.label_for(Platform::Windows), format!("Ctrl+Alt+{win}"));
            assert_eq!(
                key.accessible_label_for(Platform::Windows),
                format!("Ctrl+Alt+{spoken}")
            );
        }
        assert_eq!(
            s("Ctrl+Alt+Backspace").accessible_label_for(Platform::MacOs),
            "Control+Option+删除"
        );
        assert_eq!(
            s("Ctrl+Alt+Delete").accessible_label_for(Platform::MacOs),
            "Control+Option+向前删除"
        );
    }
    #[test]
    fn rules_cover_every_key_and_modifier_count() {
        for platform in [Platform::MacOs, Platform::Windows] {
            let reserved = if platform == Platform::MacOs {
                MAC_RESERVED
            } else {
                WIN_RESERVED
            };
            for key in KEYS {
                for prefix in ["", "Shift+", "Ctrl+", "Ctrl+Alt+"] {
                    let text = format!("{prefix}{}", key.name);
                    let shortcut = s(&text);
                    let supported = platform == Platform::Windows || key.mac.is_some();
                    let enough = key.class == KeyClass::ExtendedFunction
                        || prefix == "Ctrl+Alt+"
                        || (matches!(key.class, KeyClass::Space | KeyClass::Function)
                            && prefix == "Ctrl+");
                    let expected = supported && enough && !reserved.iter().any(|(k, _)| *k == text);
                    assert_eq!(
                        shortcut.validate_for(platform).is_ok(),
                        expected,
                        "{platform:?} {text}"
                    );
                }
            }
        }
        assert!(
            s("Super+F13")
                .validate_for(Platform::Windows)
                .unwrap_err()
                .contains("徽标键")
        );
        assert!(s("Alt+Super+KeyK").validate_for(Platform::MacOs).is_ok());
    }
    #[test]
    fn reserved_lists_are_complete_and_exact() {
        assert_eq!(MAC_RESERVED.len(), 31);
        assert_eq!(WIN_RESERVED.len(), 5);
        for (platform, list) in [
            (Platform::MacOs, MAC_RESERVED),
            (Platform::Windows, WIN_RESERVED),
        ] {
            for (key, purpose) in list {
                assert_eq!(s(key).canonical(), *key);
                assert!(
                    s(key).validate_for(platform).unwrap_err().contains(purpose),
                    "{key}"
                );
            }
        }
        assert!(
            s("Ctrl+Alt+Shift+Space")
                .validate_for(Platform::MacOs)
                .is_ok()
        );
        assert!(
            s("Ctrl+Alt+Shift+Delete")
                .validate_for(Platform::Windows)
                .is_ok()
        );
    }
    fn input(code: &str, key_code: u32) -> KeyInput {
        KeyInput {
            code: code.into(),
            key_code,
            ctrl: true,
            alt: true,
            shift: false,
            meta: false,
        }
    }
    #[test]
    fn key_input_respects_platform_mapping_and_exclusions() {
        assert_eq!(
            PanelShortcut::from_key_input(&input("KeyQ", 0x41), Platform::Windows)
                .unwrap()
                .code,
            Code::KeyA
        );
        assert_eq!(
            PanelShortcut::from_key_input(&input("KeyQ", 0x41), Platform::MacOs)
                .unwrap()
                .code,
            Code::KeyQ
        );
        for (code, vk, expected) in [
            ("NumpadEnter", 0x0d, Code::Enter),
            ("Numpad7", 0x24, Code::Home),
            ("KeyE", 0, Code::KeyE),
        ] {
            assert_eq!(
                PanelShortcut::from_key_input(&input(code, vk), Platform::Windows)
                    .unwrap()
                    .code,
                expected
            );
        }
        assert!(PanelShortcut::from_key_input(&input("KeyE", 255), Platform::Windows).is_err());
        for platform in [Platform::MacOs, Platform::Windows] {
            assert!(
                PanelShortcut::from_key_input(&input("KeyE", 229), platform)
                    .unwrap_err()
                    .contains("英文")
            );
        }
        for vk in [16, 17, 18, 91, 92, 160, 161, 162, 163, 164, 165] {
            assert!(
                PanelShortcut::from_key_input(&input("KeyA", vk), Platform::Windows)
                    .unwrap_err()
                    .contains("修饰键")
            );
        }
        for code in [
            "Escape",
            "Tab",
            "CapsLock",
            "NumLock",
            "ScrollLock",
            "PrintScreen",
            "Numpad1",
            "AudioVolumeUp",
            "MediaPlayPause",
            "IntlYen",
            "ControlLeft",
            "Fn",
            "Pause",
            "ContextMenu",
        ] {
            assert!(
                PanelShortcut::from_key_input(&input(code, 0), Platform::MacOs).is_err(),
                "{code}"
            );
        }
    }
    #[test]
    fn startup_failure_preserves_configuration() {
        let mut m = ShortcutMachine::default();
        let r = Fake::default();
        r.respond(&[Err("occupied")]);
        m.startup(&r, old());
        assert_eq!(m.registered, None);
        assert!(m.error.as_ref().unwrap().contains("occupied"));
    }
    #[test]
    fn disabled_startup_does_not_register() {
        let mut m = ShortcutMachine::default();
        let r = Fake::default();
        m.startup(&r, None);
        assert!(r.calls.borrow().is_empty());
        assert_eq!(m.revision, 1);
    }
    #[test]
    fn apply_switches_and_persists_after_registration() {
        let (mut m, r) = started();
        m.apply(&r, old(), new(), |v| {
            assert_eq!(v.as_deref(), Some("Ctrl+Alt+KeyK"));
            assert_eq!(r.calls.borrow().len(), 2);
            Ok(())
        })
        .unwrap();
        assert_eq!(m.registered, new());
        assert!(m.error.is_none());
    }
    #[test]
    fn invalid_request_does_not_mutate() {
        let (mut m, r) = started();
        let revision = m.revision;
        assert!(m.apply(&r, old(), Some(s("KeyE")), |_| panic!()).is_err());
        assert_eq!(m.revision, revision);
        assert!(r.calls.borrow().is_empty());
    }
    #[test]
    fn unregister_failure_does_not_persist() {
        let (mut m, r) = started();
        r.respond(&[Err("busy")]);
        assert!(
            m.apply(&r, old(), new(), |_| panic!())
                .unwrap_err()
                .contains("停用")
        );
        assert_eq!(m.registered, old());
    }
    #[test]
    fn registration_failure_restores_old() {
        let (mut m, r) = started();
        r.respond(&[Ok(()), Err("occupied"), Ok(())]);
        assert!(m.apply(&r, old(), new(), |_| panic!()).is_err());
        assert_eq!(m.registered, old());
        assert!(m.error.is_none());
    }
    #[test]
    fn failed_restore_is_reported() {
        let (mut m, r) = started();
        r.respond(&[Ok(()), Err("occupied"), Err("restore")]);
        assert!(
            m.apply(&r, old(), new(), |_| panic!())
                .unwrap_err()
                .contains("也没能恢复")
        );
        assert_eq!(m.registered, None);
        assert!(m.error.unwrap().contains("restore"));
    }
    #[test]
    fn persist_failure_rolls_back() {
        let (mut m, r) = started();
        assert!(
            m.apply(&r, old(), new(), |_| Err("disk".into()))
                .unwrap_err()
                .contains("disk")
        );
        assert_eq!(m.registered, old());
        assert_eq!(r.calls.borrow().len(), 4);
    }
    #[test]
    fn disable_and_unchanged_retry() {
        let (mut m, r) = started();
        m.apply(&r, old(), old(), |_| panic!()).unwrap();
        assert!(r.calls.borrow().is_empty());
        m.apply(&r, old(), None, |v| {
            assert_eq!(v, None);
            Ok(())
        })
        .unwrap();
        assert_eq!(m.registered, None);
    }
    #[test]
    fn retry_failure_refreshes_error_then_recovers() {
        let mut m = ShortcutMachine::default();
        let r = Fake::default();
        r.respond(&[Err("first"), Err("latest"), Ok(())]);
        m.startup(&r, old());
        assert!(m.apply(&r, old(), old(), |_| panic!()).is_err());
        assert!(m.error.as_ref().unwrap().contains("latest"));
        m.apply(&r, old(), old(), |_| panic!()).unwrap();
        assert!(m.error.is_none());
        assert_eq!(m.registered, old());
    }
    #[test]
    fn recording_unregisters_and_cancel_restores_with_session_guard() {
        let (mut m, r) = started();
        let first = m.begin_recording(&r).unwrap();
        assert_eq!(m.registered, None);
        assert_eq!(m.active_hotkey_id(old()), None);
        let second = m.begin_recording(&r).unwrap();
        let revision = m.revision;
        assert!(!m.end_recording(&r, Some(first), old()));
        assert_eq!(m.revision, revision);
        assert!(m.end_recording(&r, Some(second), old()));
        assert_eq!(m.registered, old());
        assert_eq!(
            m.active_hotkey_id(old()),
            Some(old().unwrap().hotkey().id())
        );
        assert!(!m.end_recording(&r, None, old()));
    }
    #[test]
    fn recording_failure_stays_paused_and_success_ends_session() {
        let (mut m, r) = started();
        m.begin_recording(&r).unwrap();
        r.respond(&[Err("occupied")]);
        assert!(m.apply(&r, old(), new(), |_| panic!()).is_err());
        assert!(m.recording.is_some());
        assert_eq!(m.registered, None);
        m.apply(&r, old(), old(), |_| panic!()).unwrap();
        assert!(m.recording.is_none());
        assert_eq!(m.registered, old());
    }
    #[test]
    fn recording_persist_failure_keeps_pause() {
        let (mut m, r) = started();
        m.begin_recording(&r).unwrap();
        assert!(m.apply(&r, old(), new(), |_| Err("disk".into())).is_err());
        assert_eq!(m.registered, None);
        assert!(m.recording.is_some());
    }
    #[test]
    fn recording_unregister_aborts_and_cancel_register_errors_are_visible() {
        let (mut m, r) = started();
        r.respond(&[Err("busy")]);
        assert!(m.begin_recording(&r).unwrap_err().contains("busy"));
        assert_eq!(m.registered, old());
        assert_eq!(m.recording, None);
        assert_eq!(
            m.active_hotkey_id(old()),
            Some(old().unwrap().hotkey().id())
        );
        let id = m.begin_recording(&r).unwrap();
        r.respond(&[Err("occupied")]);
        m.end_recording(&r, Some(id), old());
        assert!(m.error.as_ref().unwrap().contains("occupied"));
        assert_eq!(m.recording, None);
        assert_eq!(m.active_hotkey_id(old()), None);
    }
    #[test]
    fn rollback_unregister_failure_disables_leaked_hotkey_and_is_not_hidden() {
        let (mut m, r) = started();
        r.respond(&[Ok(()), Ok(()), Err("busy")]);
        assert!(
            m.apply(&r, old(), new(), |_| Err("disk".into()))
                .unwrap_err()
                .contains("busy")
        );
        assert_eq!(m.registered, new());
        assert_eq!(m.active_hotkey_id(old()), None);
        assert!(m.error.is_some());
    }
}
