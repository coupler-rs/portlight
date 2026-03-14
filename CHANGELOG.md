# Changelog

## 0.0.2

- `Window::present` now handles resizing on macOS.
- Removed task abstraction (`Task`, `TaskHandle`, `Context`, `Key`). Event handlers are now simply `FnMut()`s.

## 0.0.1

- Initial release.
