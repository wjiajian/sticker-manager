export type Platform = "macos" | "windows";

// 只支持 macOS 和 Windows；WebView2 的 UA 里带 "Windows"，其余都按 macOS 处理
export const platform: Platform = navigator.userAgent.includes("Windows") ? "windows" : "macos";
