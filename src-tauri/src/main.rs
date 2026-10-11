// Grove Lite: a very minimal browser.
//
// Layout of the single window, top to bottom:
//   tab strip + address bar   one small webview ("chrome", ui/index.html)
//   page                      one child webview per open tab ("tab-N"), or the
//                             built-in history page ("history"). A tab that has
//                             never loaded anything has no webview at all; the
//                             window background shows through.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod browser;

fn main() {
    browser::run();
}
